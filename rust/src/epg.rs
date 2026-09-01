use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

type ServiceKey = (u16, u16);

#[derive(Clone, Default)]
pub struct EpgStore {
    snapshot: Arc<RwLock<Arc<EpgSnapshot>>>,
}

#[derive(Default)]
pub struct EpgSnapshot {
    pub services: Vec<Service>,
    programs_by_service: HashMap<ServiceKey, Vec<Program>>,
    #[expect(dead_code, reason = "used by the upcoming EPG refresh scheduler")]
    pub synced_at: u64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: u64,
    pub service_id: u16,
    pub network_id: u16,
    pub name: String,
    #[serde(rename = "type")]
    pub service_type: u16,
    pub remote_control_key_id: Option<u16>,
    pub channel: ServiceChannel,
}

#[derive(Clone, Deserialize)]
pub struct ServiceChannel {
    #[serde(rename = "type")]
    pub channel_type: String,
    pub channel: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    #[expect(dead_code, reason = "used by the upcoming program guide model")]
    pub id: u64,
    pub event_id: u16,
    pub service_id: u16,
    pub network_id: u16,
    pub start_at: u64,
    pub duration: u64,
    pub name: Option<String>,
    #[expect(dead_code, reason = "used by the upcoming program detail view")]
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct CurrentProgram {
    pub event_id: u16,
    pub service_id: u16,
    pub network_id: u16,
    pub start_at: u64,
    pub duration: u64,
    pub name: Option<String>,
}

impl EpgStore {
    pub fn replace(&self, services: Vec<Service>, programs: Vec<Program>, synced_at: u64) {
        let mut programs_by_service = HashMap::<ServiceKey, Vec<Program>>::new();
        for program in programs {
            programs_by_service
                .entry((program.network_id, program.service_id))
                .or_default()
                .push(program);
        }
        for schedule in programs_by_service.values_mut() {
            schedule.sort_unstable_by_key(|program| program.start_at);
        }
        let replacement = Arc::new(EpgSnapshot {
            services,
            programs_by_service,
            synced_at,
        });
        *self
            .snapshot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = replacement;
    }

    pub fn snapshot(&self) -> Arc<EpgSnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl EpgSnapshot {
    pub fn current_programs(&self, now: u64) -> Vec<CurrentProgram> {
        self.programs_by_service
            .values()
            .filter_map(|schedule| {
                let index = schedule.partition_point(|program| program.start_at <= now);
                index.checked_sub(1).and_then(|current| {
                    let program = &schedule[current];
                    (program.start_at.saturating_add(program.duration) > now).then(|| {
                        CurrentProgram {
                            event_id: program.event_id,
                            service_id: program.service_id,
                            network_id: program.network_id,
                            start_at: program.start_at,
                            duration: program.duration,
                            name: program.name.clone(),
                        }
                    })
                })
            })
            .collect()
    }

    #[allow(dead_code, reason = "query boundary for the upcoming program guide")]
    pub fn programs_between(&self, start_at: u64, end_at: u64) -> Vec<&Program> {
        self.programs_by_service
            .values()
            .flat_map(|schedule| schedule.iter())
            .filter(|program| {
                program.start_at < end_at
                    && program.start_at.saturating_add(program.duration) > start_at
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(start_at: u64, duration: u64) -> Program {
        Program {
            id: 1,
            event_id: 10,
            service_id: 20,
            network_id: 30,
            start_at,
            duration,
            name: Some("News".to_owned()),
            description: None,
        }
    }

    #[test]
    fn replaces_snapshots_and_finds_current_program() {
        let store = EpgStore::default();
        store.replace(Vec::new(), vec![program(1_000, 500)], 900);
        let first = store.snapshot();
        assert_eq!(first.current_programs(1_250).len(), 1);

        store.replace(Vec::new(), vec![program(2_000, 500)], 1_900);
        let second = store.snapshot();
        assert_eq!(second.current_programs(2_250).len(), 1);
        assert_eq!(first.current_programs(1_250).len(), 1);
    }

    #[test]
    fn queries_program_guide_ranges() {
        let store = EpgStore::default();
        store.replace(
            Vec::new(),
            vec![program(1_000, 500), program(2_000, 500)],
            900,
        );
        assert_eq!(store.snapshot().programs_between(1_400, 2_100).len(), 2);
    }
}
