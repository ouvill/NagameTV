//! Extract only a selected service's component tags from native PMT bus messages.
use gstreamer as gst;
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PMTの長さまたは構造が不正です")]
    Structure,
    #[error("PMTのCRCが不正です")]
    Checksum,
    #[error("複数sectionのPMTには対応していません")]
    MultipleSections,
    #[error("PMTのPIDまたはcomponent_tagが重複しています")]
    Ambiguous,
}

#[derive(Debug, PartialEq, Eq)]
struct Component {
    pid: u16,
    tag: u8,
}

#[derive(Default)]
pub struct Components {
    service: Option<u16>,
    entries: Vec<Component>,
}
impl Components {
    pub fn for_service(service: Option<u16>) -> Self {
        Self {
            service,
            entries: Vec::new(),
        }
    }

    pub fn observe(&mut self, message: &gst::MessageRef) -> Result<(), Error> {
        if !matches!(message.view(), gst::MessageView::Element(_))
            || self.service.is_none()
            || !message
                .structure()
                .is_some_and(|structure| structure.has_field("section"))
        {
            return Ok(());
        }
        let Some(mut section) = gstreamer_mpegts::message_parse_mpegts_section(&message.to_owned())
        else {
            return Ok(());
        };
        let data = section.data();
        self.update(data.as_ref())
    }

    fn update(&mut self, section: &[u8]) -> Result<(), Error> {
        let Some(header) = section.get(..5) else {
            return Ok(());
        };
        if header[0] != 2 || self.service != Some(u16::from_be_bytes([header[3], header[4]])) {
            return Ok(());
        }
        match parse(section) {
            Ok(Some(entries)) => {
                if self.entries != entries {
                    eprintln!(
                        "AUDIO_PMT service={} components={entries:?}",
                        u16::from_be_bytes([header[3], header[4]])
                    );
                    self.entries = entries;
                }
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => {
                self.entries.clear();
                Err(error)
            }
        }
    }

    pub fn tag_for_stream(&self, id: &str) -> Option<u8> {
        // Gst mpegtsbase IDs end with an eight-digit hexadecimal ES PID.
        let suffix = id.rsplit_once('/')?.1;
        if suffix.len() != 8 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let pid = u16::from_str_radix(suffix, 16).ok()?;
        self.entries
            .iter()
            .find(|entry| entry.pid == pid)
            .map(|entry| entry.tag)
    }
}

fn crc(bytes: &[u8]) -> u32 {
    bytes.iter().fold(u32::MAX, |mut crc, byte| {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    0x04c1_1db7
                } else {
                    0
                };
        }
        crc
    })
}

fn parse(section: &[u8]) -> Result<Option<Vec<Component>>, Error> {
    if !(16..=1024).contains(&section.len())
        || section[0] != 2
        || section[1] & 0xf0 != 0xb0
        || 3 + ((usize::from(section[1] & 15) << 8) | usize::from(section[2])) != section.len()
    {
        return Err(Error::Structure);
    }
    if crc(section) != 0 {
        return Err(Error::Checksum);
    }
    if section[5] & 1 == 0 {
        return Ok(None);
    }
    if section[6] != 0 || section[7] != 0 {
        return Err(Error::MultipleSections);
    }
    let end = section.len() - 4;
    let mut pos = 12 + ((usize::from(section[10] & 15) << 8) | usize::from(section[11]));
    if pos > end {
        return Err(Error::Structure);
    }
    let mut entries: Vec<Component> = Vec::new();
    let mut pids = HashSet::new();
    while pos < end {
        let head = section
            .get(pos..pos + 5)
            .filter(|_| pos + 5 <= end)
            .ok_or(Error::Structure)?;
        let pid = (u16::from(head[1] & 31) << 8) | u16::from(head[2]);
        if !pids.insert(pid) {
            return Err(Error::Ambiguous);
        }
        let next = pos + 5 + ((usize::from(head[3] & 15) << 8) | usize::from(head[4]));
        if next > end {
            return Err(Error::Structure);
        }
        let mut descriptor = pos + 5;
        let mut tag = None;
        while descriptor < next {
            if descriptor + 2 > next {
                return Err(Error::Structure);
            }
            let size = usize::from(section[descriptor + 1]);
            if descriptor + 2 + size > next {
                return Err(Error::Structure);
            }
            if section[descriptor] == 0x52 {
                if size != 1 {
                    return Err(Error::Structure);
                }
                if tag.replace(section[descriptor + 2]).is_some() {
                    return Err(Error::Ambiguous);
                }
            }
            descriptor += 2 + size;
        }
        if let Some(tag) = tag {
            if entries.iter().any(|entry| entry.tag == tag) {
                return Err(Error::Ambiguous);
            }
            entries.push(Component { pid, tag });
        }
        pos = next;
    }
    Ok(Some(entries))
}

#[cfg(test)]
mod tests;
