use super::wire::{self, Pid, PsiSection, STUFFING_BYTE};
use std::collections::BTreeMap;

/// Incomplete data belongs to exactly one PID. Callers reset on discontinuity.
#[derive(Default)]
pub(crate) struct Sections {
    pending: Option<Vec<u8>>,
    si: bool,
}

impl Sections {
    pub(super) fn si() -> Self {
        Self {
            pending: None,
            si: true,
        }
    }
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.as_ref().is_none_or(Vec::is_empty)
    }
    pub(crate) fn push(&mut self, start: bool, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut complete = Vec::new();
        if payload.is_empty() {
            return complete;
        }
        if start {
            let pointer = usize::from(payload[0]);
            if pointer >= payload.len() {
                self.pending = None;
                return complete;
            }
            self.append(&payload[1..1 + pointer], &mut complete);
            self.pending = Some(payload[1 + pointer..].to_vec());
        } else if let Some(pending) = &mut self.pending {
            pending.extend_from_slice(payload);
        }
        self.append(&[], &mut complete);
        complete
    }

    fn append(&mut self, bytes: &[u8], complete: &mut Vec<Vec<u8>>) {
        if let Some(pending) = &mut self.pending {
            pending.extend_from_slice(bytes);
        }
        while let Some(data) = &mut self.pending {
            if data.is_empty() || data.first() == Some(&STUFFING_BYTE) {
                self.pending = None;
                break;
            }
            let size = match if self.si {
                super::programs::section_size(data)
            } else {
                wire::section_size(data)
            } {
                Ok(size) => size,
                Err(wire::ParseError::Incomplete) => break,
                Err(wire::ParseError::Invalid(_)) => {
                    self.pending = None;
                    break;
                }
            };
            if data.len() < size {
                break;
            }
            complete.push(data.drain(..size).collect());
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
