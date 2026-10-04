//! Resource extraction from completed carousel modules (STD-B24, Part 2, §9.1.2).

use crate::{DecodedModule, Error};
use std::borrow::Cow;
use std::io::Write;
use std::path::Path;

const TYPE_DESCRIPTOR: u8 = 0x01;
const NAME_DESCRIPTOR: u8 = 0x02;
const MAX_HEADER_BYTES: usize = 16 * 1024;

/// The two resource-to-module mappings in STD-B24 Volume 2 §9.1.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceMapping {
    Direct,
    Entity,
}

/// One resource ready for a BML interpreter or an explicitly chosen save path.
/// Names are bytes because their character encoding depends on the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub mapping: ResourceMapping,
    /// Present when this resource was discovered through a service PMT.
    pub component_tag: Option<u8>,
    pub download_id: u32,
    pub module_id: u16,
    pub module_version: u8,
    pub module_name: Vec<u8>,
    pub name: Vec<u8>,
    pub media_type: Option<Vec<u8>>,
    pub data: Vec<u8>,
}

impl Resource {
    /// Writes the exact decoded bytes to a new caller-selected path.
    /// Broadcast names are never interpreted as filesystem paths here.
    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&self.data)
    }
}

impl DecodedModule {
    /// Resolves a direct-mapped module or extracts resources from its MIME entity.
    pub fn resources(self) -> Result<Vec<Resource>, Error> {
        if self.info.module_link()?.is_some() {
            return Err(Error::LinkedModuleFragment);
        }
        let descriptor = |tag| {
            let mut values = self.info.descriptors.iter().filter(|item| item.tag == tag);
            let first = values.next();
            if values.next().is_some() {
                Err(Error::Invalid("duplicate module descriptor"))
            } else {
                Ok(first.map(|item| item.data.as_slice()))
            }
        };
        let module_name = descriptor(NAME_DESCRIPTOR)?.map_or_else(
            || format!("{:04X}", self.info.id).into_bytes(),
            ToOwned::to_owned,
        );
        let module_type = descriptor(TYPE_DESCRIPTOR)?.map(ToOwned::to_owned);
        let base = Resource {
            mapping: ResourceMapping::Entity,
            component_tag: None,
            download_id: self.download_id,
            module_id: self.info.id,
            module_version: self.info.version,
            module_name: module_name.clone(),
            name: module_name,
            media_type: module_type,
            data: Vec::new(),
        };
        let declared_multipart = base
            .media_type
            .as_deref()
            .is_some_and(|kind| media_type(kind).eq_ignore_ascii_case(b"multipart/mixed"));
        let declared_direct = base.media_type.is_some() && !declared_multipart;
        if declared_multipart && !looks_like_entity(&self.data) {
            return Err(Error::Invalid("multipart module entity"));
        }
        if declared_direct || !looks_like_entity(&self.data) {
            return Ok(vec![Resource {
                mapping: ResourceMapping::Direct,
                data: self.data,
                ..base
            }]);
        }
        let (headers, body) = parse_headers(&self.data)?;
        let content_type = required_header(&headers, b"content-type")?;
        if declared_multipart && !media_type(content_type).eq_ignore_ascii_case(b"multipart/mixed")
        {
            return Err(Error::Invalid("multipart module content type"));
        }
        if media_type(content_type).eq_ignore_ascii_case(b"multipart/mixed") {
            let boundary = boundary(content_type)?;
            parse_multipart(body, boundary, base)
        } else {
            let name = header(&headers, b"content-location")?
                .map_or_else(|| base.module_name.clone(), ToOwned::to_owned);
            let data = entity_body(body, &headers)?.to_vec();
            Ok(vec![Resource {
                name,
                media_type: Some(content_type.to_vec()),
                data,
                ..base
            }])
        }
    }
}

fn looks_like_entity(data: &[u8]) -> bool {
    // STD-B24 §9.1.2 uses HTTP entity headers; their order is not fixed.
    let data = &data[..data.len().min(MAX_HEADER_BYTES)];
    data.split(|byte| *byte == b'\n')
        .take_while(|line| !line.is_empty() && *line != b"\r")
        .any(|line| {
            line.get(..13)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"Content-Type:"))
                || line
                    .get(..17)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"Content-Location:"))
        })
}

type Headers<'a> = Vec<(&'a [u8], Cow<'a, [u8]>)>;

fn parse_headers(data: &[u8]) -> Result<(Headers<'_>, &[u8]), Error> {
    let end = data
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(Error::Invalid("entity header terminator"))?;
    if end > MAX_HEADER_BYTES {
        return Err(Error::Invalid("entity header too large"));
    }
    let mut headers: Headers<'_> = Vec::new();
    let mut lines = &data[..end];
    while !lines.is_empty() {
        let (line, rest) = match lines.windows(2).position(|window| window == b"\r\n") {
            Some(end) => (&lines[..end], &lines[end + 2..]),
            None => (lines, &[][..]),
        };
        lines = rest;
        // RFC2068 §2.2 permits continuation lines starting with SP or HTAB.
        if matches!(line.first(), Some(b' ' | b'\t')) {
            let (_, value) = headers
                .last_mut()
                .ok_or(Error::Invalid("orphan entity header continuation"))?;
            let continuation = trim_ascii(line);
            validate_header_value(continuation)?;
            value.to_mut().push(b' ');
            value.to_mut().extend_from_slice(continuation);
            continue;
        }
        let colon = line
            .iter()
            .position(|byte| *byte == b':')
            .ok_or(Error::Invalid("entity header field"))?;
        let name = &line[..colon];
        if name.is_empty()
            || !name
                .iter()
                .all(|byte| byte.is_ascii_graphic() && !b"()<>@,;:\\\"/[]?={}".contains(byte))
        {
            return Err(Error::Invalid("entity header name"));
        }
        let value = trim_ascii(&line[colon + 1..]);
        validate_header_value(value)?;
        headers.push((name, Cow::Borrowed(value)));
    }
    Ok((headers, &data[end + 4..]))
}

fn validate_header_value(value: &[u8]) -> Result<(), Error> {
    if value
        .iter()
        .any(|byte| (*byte < 0x20 && *byte != b'\t') || *byte == 0x7f)
    {
        return Err(Error::Invalid("entity header value"));
    }
    Ok(())
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

fn header<'a>(headers: &'a Headers<'_>, name: &[u8]) -> Result<Option<&'a [u8]>, Error> {
    let mut values = headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name));
    let first = values.next();
    if values.next().is_some() {
        return Err(Error::Invalid("duplicate entity header"));
    }
    Ok(first.map(|(_, value)| value.as_ref()))
}

fn required_header<'a>(headers: &'a Headers<'_>, name: &[u8]) -> Result<&'a [u8], Error> {
    header(headers, name)?.ok_or(Error::Invalid("missing entity header"))
}

fn media_type(value: &[u8]) -> &[u8] {
    trim_ascii(value.split(|byte| *byte == b';').next().unwrap_or_default())
}

fn boundary(content_type: &[u8]) -> Result<&[u8], Error> {
    for parameter in content_type.split(|byte| *byte == b';').skip(1) {
        let parameter = trim_ascii(parameter);
        let Some(equal) = parameter.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        if !trim_ascii(&parameter[..equal]).eq_ignore_ascii_case(b"boundary") {
            continue;
        }
        let raw = trim_ascii(&parameter[equal + 1..]);
        let value = raw
            .strip_prefix(b"\"")
            .and_then(|rest| rest.strip_suffix(b"\""))
            .unwrap_or(raw);
        if value.is_empty()
            || value.len() > 70
            || value
                .iter()
                .any(|byte| !byte.is_ascii_graphic() || *byte == b'"')
        {
            return Err(Error::Invalid("multipart boundary"));
        }
        return Ok(value);
    }
    Err(Error::Invalid("missing multipart boundary"))
}

fn entity_body<'a>(body: &'a [u8], headers: &Headers<'_>) -> Result<&'a [u8], Error> {
    let Some(length) = header(headers, b"content-length")? else {
        return Ok(body);
    };
    let length = std::str::from_utf8(length)
        .map_err(|_| Error::Invalid("content length"))?
        .parse::<usize>()
        .map_err(|_| Error::Invalid("content length"))?;
    if body.len() != length {
        return Err(Error::Invalid("content length"));
    }
    Ok(body)
}

fn parse_multipart(
    mut body: &[u8],
    boundary: &[u8],
    base: Resource,
) -> Result<Vec<Resource>, Error> {
    let mut delimiter = Vec::with_capacity(boundary.len() + 4);
    delimiter.extend_from_slice(b"--");
    delimiter.extend_from_slice(boundary);
    let first = body
        .windows(delimiter.len())
        .position(|window| window == delimiter)
        .ok_or(Error::Invalid("multipart opening boundary"))?;
    if first != 0 && !body[..first].ends_with(b"\r\n") {
        return Err(Error::Invalid("multipart preamble"));
    }
    body = &body[first..];
    let mut resources = Vec::new();
    loop {
        if !body.starts_with(&delimiter) {
            return Err(Error::Invalid("multipart boundary"));
        }
        body = &body[delimiter.len()..];
        if let Some(rest) = body.strip_prefix(b"--") {
            if !rest.is_empty() && !rest.starts_with(b"\r\n") {
                return Err(Error::Invalid("multipart closing boundary"));
            }
            return Ok(resources);
        }
        body = body
            .strip_prefix(b"\r\n")
            .ok_or(Error::Invalid("multipart boundary line"))?;
        let (headers, part) = parse_headers(body)?;
        let kind = required_header(&headers, b"content-type")?;
        let name = required_header(&headers, b"content-location")?;
        let mut separator = Vec::with_capacity(delimiter.len() + 2);
        separator.extend_from_slice(b"\r\n");
        separator.extend_from_slice(&delimiter);
        let length = if let Some(value) = header(&headers, b"content-length")? {
            std::str::from_utf8(value)
                .map_err(|_| Error::Invalid("content length"))?
                .parse::<usize>()
                .map_err(|_| Error::Invalid("content length"))?
        } else {
            part.windows(separator.len())
                .position(|window| window == separator)
                .ok_or(Error::Invalid("multipart next boundary"))?
        };
        let (data, rest) = part
            .split_at_checked(length)
            .ok_or(Error::Truncated("resource body"))?;
        if !rest.starts_with(&separator) {
            return Err(Error::Invalid("multipart next boundary"));
        }
        body = &rest[2..]; // Keep the delimiter for the next iteration.
        resources.push(Resource {
            name: name.to_vec(),
            media_type: Some(kind.to_vec()),
            data: data.to_vec(),
            ..base.clone()
        });
    }
}
