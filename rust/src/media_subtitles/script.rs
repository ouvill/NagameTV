use super::{Error, HEADER};
use std::{path::Path, sync::Arc};

pub(super) const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    pub(super) name: String,
    pub(super) bytes: Arc<[u8]>,
}

impl Script {
    pub(super) fn load(path: &Path) -> Result<Self, Error> {
        use std::io::Read;
        if !std::fs::metadata(path)?.is_file() {
            return Err(Error::Format);
        }
        let file = std::fs::File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::Format);
        }
        let mut bytes = Vec::new();
        file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Encoding)?
            .trim_start_matches('\u{feff}');
        let text = text.replace("\r\n", "\n");
        let script = match path
            .extension()
            .and_then(|v| v.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("ass" | "ssa") if text.contains("[Script Info]") && text.contains("[Events]") => {
                text
            }
            Some("srt") => srt(&text)?,
            _ => return Err(Error::Format),
        };
        if script.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        // Validate off the UI thread, before replacing a working subtitle track.
        let mut renderer = super::renderer::ffi::make_renderer()?;
        renderer.pin_mut().script(script.as_bytes())?;
        Ok(Self {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            bytes: script.into_bytes().into(),
        })
    }
}

fn timestamp(text: &str) -> Option<u64> {
    let parts: Vec<_> = text.trim().split([':', ',', '.']).collect();
    if parts.len() != 4 {
        return None;
    }
    let hour: u64 = parts[0].parse().ok()?;
    let minute: u64 = parts[1].parse().ok()?;
    let second: u64 = parts[2].parse().ok()?;
    let ms: u64 = parts[3].parse().ok()?;
    if minute >= 60 || second >= 60 || ms >= 1000 {
        return None;
    }
    hour.checked_mul(3_600_000)?
        .checked_add(minute * 60_000 + second * 1000 + ms)
}
fn ass_time(ms: u64) -> String {
    format!(
        "{}:{:02}:{:02}.{:02}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms / 10 % 100
    )
}
fn srt(text: &str) -> Result<String, Error> {
    let mut output = HEADER.to_owned();
    let mut count = 0;
    for block in text
        .trim()
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
    {
        let mut lines = block.trim().lines();
        let first = lines.next().ok_or(Error::Format)?;
        let timing = if first.contains("-->") {
            first
        } else {
            first.trim().parse::<u64>().map_err(|_| Error::Format)?;
            lines.next().ok_or(Error::Format)?
        };
        let (start, end) = timing.split_once("-->").ok_or(Error::Format)?;
        let start = timestamp(start).ok_or(Error::Format)?;
        let end = timestamp(end)
            .filter(|end| *end > start)
            .ok_or(Error::Format)?;
        let body = lines.collect::<Vec<_>>().join("\n");
        if body.is_empty() {
            return Err(Error::Format);
        }
        count += 1;
        if count > super::MAX_EVENTS {
            return Err(Error::Capacity);
        }
        output.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
            ass_time(start),
            ass_time(end),
            plain_text(&body)
        ));
    }
    if count == 0 {
        return Err(Error::Format);
    }
    Ok(output)
}

/// Plain/Pango text is content, never an ASS command. Preserve common SRT
/// emphasis while escaping braces and backslashes before introducing tags.
pub(super) fn plain_text(text: &str) -> String {
    let mut text = text
        .replace('\\', "\\\u{2060}")
        .replace('{', "\\{")
        .replace('}', "\\}");
    for (html, ass) in [
        ("<i>", "{\\i1}"),
        ("</i>", "{\\i0}"),
        ("<b>", "{\\b1}"),
        ("</b>", "{\\b0}"),
        ("<u>", "{\\u1}"),
        ("</u>", "{\\u0}"),
    ] {
        text = text.replace(html, ass);
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace('\n', "\\N")
}
