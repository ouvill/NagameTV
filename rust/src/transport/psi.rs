use super::wire::{self, Pid, PsiSection, SECTION_PREFIX_SIZE, STUFFING_BYTE};
use std::collections::BTreeMap;

#[derive(Default)]
enum Syntax {
    #[default]
    Psi,
    Si,
}
impl Syntax {
    fn section_size(&self, bytes: &[u8]) -> Result<usize, wire::ParseError> {
        match self {
            Self::Psi => wire::section_size(bytes),
            Self::Si => super::programs::section_size(bytes),
        }
    }
}

/// Incomplete data belongs to exactly one PID. Callers reset on discontinuity.
/// Malformed sections are discarded until the next signalled start; CRC and
/// table contents are validated by callers before they update accepted state.
#[derive(Default)]
pub(crate) struct Sections {
    pending: Option<Vec<u8>>,
    syntax: Syntax,
}

impl Sections {
    pub(super) fn si() -> Self {
        Self {
            pending: None,
            syntax: Syntax::Si,
        }
    }
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
    pub(crate) fn push(&mut self, start: bool, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut complete = Vec::new();
        if payload.is_empty() {
            return complete;
        }
        if start {
            let pointer = usize::from(payload[0]);
            let Some((prefix, sections)) = payload[1..].split_at_checked(pointer) else {
                self.pending = None;
                return complete;
            };
            if sections.is_empty() {
                self.pending = None;
                return complete;
            }
            complete.extend(self.finish_pending(prefix));
            // The pointer marks a fresh boundary even if the old section was truncated.
            self.pending = None;
            self.start_sections(sections, &mut complete);
        } else {
            complete.extend(self.finish_pending(payload));
        }
        complete
    }

    // H.222.0 §2.4.4.3: bytes before a pointer boundary, or in a packet without
    // payload_unit_start_indicator, cannot begin another section.
    // https://www.itu.int/rec/T-REC-H.222.0/en
    fn finish_pending(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        let mut data = self.pending.take()?;
        let prefix = SECTION_PREFIX_SIZE
            .saturating_sub(data.len())
            .min(bytes.len());
        data.extend_from_slice(&bytes[..prefix]);
        let bytes = &bytes[prefix..];
        match self.syntax.section_size(&data) {
            Ok(size) => {
                let remaining = size - data.len();
                data.reserve_exact(remaining);
                data.extend_from_slice(&bytes[..remaining.min(bytes.len())]);
                if data.len() == size {
                    return Some(data);
                }
                self.pending = Some(data);
            }
            Err(wire::ParseError::Incomplete) => self.pending = Some(data),
            Err(wire::ParseError::Invalid(_)) => {}
        }
        None
    }

    fn start_sections(&mut self, mut bytes: &[u8], complete: &mut Vec<Vec<u8>>) {
        while !bytes.is_empty() && bytes[0] != STUFFING_BYTE {
            match self.syntax.section_size(bytes) {
                Ok(size) if bytes.len() >= size => {
                    // Copy each complete section once; never shift a packet's tail.
                    complete.push(bytes[..size].to_vec());
                    bytes = &bytes[size..];
                }
                Ok(size) => {
                    let mut pending = Vec::with_capacity(size);
                    pending.extend_from_slice(bytes);
                    self.pending = Some(pending);
                    break;
                }
                Err(wire::ParseError::Incomplete) => {
                    self.pending = Some(bytes.to_vec());
                    break;
                }
                Err(wire::ParseError::Invalid(_)) => break,
            }
        }
    }
}

/// A PAT version is usable only after all its sections have arrived.
#[derive(Default)]
pub(crate) struct Pat {
    identity: Option<(u16, u8, u8)>,
    sections: BTreeMap<u8, Vec<(u16, Pid)>>,
}

impl Pat {
    pub(crate) fn push(&mut self, section: &PsiSection<'_>) -> Option<BTreeMap<u16, Pid>> {
        let programs = section.pat_programs().ok()?;
        let identity = (
            section.extension,
            section.version,
            section.last_section_number,
        );
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.sections.clear();
        }
        self.sections.insert(section.section_number, programs);
        if self.sections.len() != usize::from(section.last_section_number) + 1 {
            return None;
        }
        let mut programs = BTreeMap::new();
        for &(service, pid) in self.sections.values().flatten() {
            if programs.insert(service, pid).is_some() {
                return None;
            }
        }
        Some(programs)
    }
}

#[cfg(test)]
mod tests;
