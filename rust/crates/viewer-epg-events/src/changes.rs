//! Bounded last-write-wins changes; losing correspondence requires reconciliation.
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PENDING_PROGRAMS: usize = 1024;
pub const MAX_PENDING_SERVICES: usize = 256;
pub const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub enum ProgramChange {
    Upsert { id: u64, data: Box<RawValue> },
    Remove(u64),
}
impl ProgramChange {
    pub fn id(&self) -> u64 {
        match self {
            Self::Upsert { id, .. } | Self::Remove(id) => *id,
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Upsert { data, .. } => data.get().len(),
            Self::Remove(_) => 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct Delta {
    programs: BTreeMap<u64, ProgramChange>,
    services: BTreeSet<u64>,
    bytes: usize,
}
impl Delta {
    pub fn into_parts(self) -> (impl Iterator<Item = ProgramChange>, BTreeSet<u64>) {
        (self.programs.into_values(), self.services)
    }
}

#[derive(Debug)]
pub enum Changes {
    Delta(Delta),
    Full,
}
impl Default for Changes {
    fn default() -> Self {
        Self::Delta(Delta::default())
    }
}
impl Changes {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Delta(delta) if delta.programs.is_empty() && delta.services.is_empty())
    }
    pub fn full(&mut self) {
        *self = Self::Full;
    }
    pub fn service(&mut self, id: u64) {
        if let Self::Delta(delta) = self {
            delta.services.insert(id);
            if delta.services.len() > MAX_PENDING_SERVICES {
                self.full();
            }
        }
    }
    pub fn program(&mut self, change: ProgramChange) {
        if let Self::Delta(delta) = self {
            let id = change.id();
            delta.bytes += change.bytes();
            if let Some(previous) = delta.programs.insert(id, change) {
                delta.bytes -= previous.bytes();
            }
            if delta.programs.len() > MAX_PENDING_PROGRAMS || delta.bytes > MAX_PENDING_BYTES {
                self.full();
            }
        }
    }
    pub fn merge(&mut self, changes: Self) {
        match changes {
            Self::Full => self.full(),
            Self::Delta(delta) => {
                let (programs, services) = delta.into_parts();
                for program in programs {
                    self.program(program);
                }
                for service in services {
                    self.service(service);
                }
            }
        }
    }
    pub(crate) fn event(
        &mut self,
        kind: &str,
        data: Option<Box<RawValue>>,
    ) -> Result<(), crate::Error> {
        #[derive(Deserialize)]
        struct Identity {
            id: u64,
        }
        if !matches!(kind, "create" | "update" | "remove") {
            self.full();
            return Ok(());
        }
        let Some(data) = data else {
            // Older/unsupported notifications can omit data; reconciliation
            // supplies the missing payload without guessing program identity.
            self.full();
            return Ok(());
        };
        // The application validates the complete program with the HTTP schema.
        let Identity { id } = serde_json::from_str(data.get())?;
        if kind == "remove" {
            self.program(ProgramChange::Remove(id));
        } else {
            self.program(ProgramChange::Upsert { id, data });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_ids_are_bounded_and_last_operation_wins() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut changes = Changes::default();
        for id in 0..10_000 {
            let data = RawValue::from_string(format!(r#"{{"id":7,"name":"{id}"}}"#))?;
            changes.event("update", Some(data))?;
        }
        changes.program(ProgramChange::Remove(7));
        let Changes::Delta(delta) = changes else {
            panic!("one ID never overflows")
        };
        assert_eq!(delta.bytes, 0);
        let (mut programs, services) = delta.into_parts();
        assert!(matches!(programs.next(), Some(ProgramChange::Remove(7))));
        assert!(programs.next().is_none());
        assert!(services.is_empty());
        Ok(())
    }

    #[test]
    fn each_backlog_limit_collapses_to_one_full_refresh() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut programs = Changes::default();
        for id in 0..=MAX_PENDING_PROGRAMS {
            programs.program(ProgramChange::Remove(id as u64));
        }
        assert!(matches!(programs, Changes::Full));
        let mut services = Changes::default();
        for id in 0..=MAX_PENDING_SERVICES {
            services.service(id as u64);
        }
        assert!(matches!(services, Changes::Full));
        let mut bytes = Changes::default();
        for id in 0..5 {
            bytes.program(ProgramChange::Upsert {
                id,
                data: RawValue::from_string(format!(
                    r#"{{"id":{id},"name":"{}"}}"#,
                    "x".repeat(MAX_PENDING_BYTES / 4)
                ))?,
            });
        }
        assert!(matches!(bytes, Changes::Full));
        bytes.program(ProgramChange::Remove(9));
        assert!(matches!(bytes, Changes::Full));
        Ok(())
    }

    #[test]
    fn decoders_preserve_program_payloads_and_coalesce_service_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut changes = Changes::default();
        crate::Decoder::default().push_changes(
            br#"[
            {"resource":"program","type":"create","data":{"id":1,"name":"first"}},
            {"resource":"program","type":"remove","data":{"id":1}},
            {"resource":"program","type":"update","data":{"id":1,"name":"last"}},
            {"resource":"service","type":"update","data":{"id":9}}
        "#,
            &mut changes,
        )?;
        let Changes::Delta(delta) = changes else {
            panic!("valid operations")
        };
        let (mut programs, _) = delta.into_parts();
        match programs.next().expect("upsert") {
            ProgramChange::Upsert { data, .. } => assert!(data.get().contains("last")),
            ProgramChange::Remove(_) => panic!("last upsert must survive"),
        }
        assert!(programs.next().is_none());
        let mut changes = Changes::default();
        let mut decoder = crate::sse::Decoder::default();
        for _ in 0..100 {
            decoder.push_changes(
                b"event: epg.programs-updated\ndata: {\"serviceId\":9000000000}\n\n",
                &mut changes,
            )?;
        }
        let Changes::Delta(delta) = changes else {
            panic!("one service")
        };
        assert_eq!(delta.into_parts().1, BTreeSet::from([9_000_000_000]));
        Ok(())
    }
}
