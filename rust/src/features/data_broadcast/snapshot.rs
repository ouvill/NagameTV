//! Latest receive state for a new browser subscriber. Events are never replayed.
//! Retention is an application policy; ARIB parsing stays in arib-b24.
use super::{ServiceIdentity, Update, metadata};
use arib_b24::{DownloadInfo, ModuleInfo, ModuleLink, transport::ServiceUpdate};
use std::{collections::BTreeMap, sync::Arc};

// Aggregate retained resources, not a limit on a standards-defined module size.
const RESOURCE_BUDGET: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("retained data broadcast resources exceed {limit} bytes")]
    Capacity { limit: usize },
    #[error("invalid data broadcast snapshot: {0}")]
    Invalid(&'static str),
    #[error("data broadcast snapshot descriptor: {0}")]
    Descriptor(#[from] arib_b24::Error),
}

struct Module {
    update: Arc<Update>,
    dependencies: Vec<ModuleInfo>,
    bytes: usize,
}
struct Component {
    list: Arc<Update>,
    modules: BTreeMap<u16, Module>,
}
impl Component {
    fn info(&self) -> &DownloadInfo {
        match self.list.as_ref() {
            Update::Broadcast(ServiceUpdate::ModuleList { info, .. }) => info,
            _ => unreachable!("only module lists construct Component"),
        }
    }
}

pub(super) struct Snapshot {
    program: Arc<Update>,
    time: Option<Arc<Update>>,
    clock: Option<Arc<Update>>,
    broadcasters: Option<Arc<Update>>,
    components: Option<Arc<Update>>,
    carousels: BTreeMap<u8, Component>,
    budget: usize,
}
impl Snapshot {
    pub fn new(identity: ServiceIdentity) -> Self {
        Self {
            program: Arc::new(Update::Metadata(metadata::Update::Program(
                viewer_web_bml::ProgramInfo {
                    service_id: identity.service_id,
                    original_network_id: identity.original_network_id,
                    transport_stream_id: None,
                    event: None,
                },
            ))),
            time: None,
            clock: None,
            broadcasters: None,
            components: None,
            carousels: BTreeMap::new(),
            budget: RESOURCE_BUDGET,
        }
    }

    pub fn apply(&mut self, update: Arc<Update>) -> Result<(), Error> {
        match update.as_ref() {
            Update::Metadata(metadata::Update::Program(program)) => {
                if let Update::Metadata(metadata::Update::Program(previous)) = self.program.as_ref()
                    && previous.transport_stream_id != program.transport_stream_id
                {
                    self.time = None;
                    self.clock = None;
                }
                self.program = update;
            }
            Update::Metadata(metadata::Update::Time(_)) => self.time = Some(update),
            Update::Metadata(metadata::Update::Clock { .. }) => self.clock = Some(update),
            Update::Broadcast(ServiceUpdate::Broadcasters(_)) => self.broadcasters = Some(update),
            Update::Broadcast(ServiceUpdate::Components(components)) => {
                self.carousels
                    .retain(|_, retained| match retained.list.as_ref() {
                        Update::Broadcast(ServiceUpdate::ModuleList { component, .. }) => {
                            components.contains(component)
                        }
                        _ => unreachable!("only module lists construct Component"),
                    });
                self.components = Some(update);
            }
            Update::Broadcast(ServiceUpdate::ModuleList { component, info }) => {
                if let Some(previous) = self.carousels.get_mut(&component.tag()) {
                    let same_download = previous.info().download_id == info.download_id;
                    previous.modules.retain(|_, module| {
                        same_download
                            && module
                                .dependencies
                                .iter()
                                .all(|old| info.modules.iter().any(|current| current == old))
                    });
                    previous.list = update;
                } else {
                    self.carousels.insert(
                        component.tag(),
                        Component {
                            list: update,
                            modules: BTreeMap::new(),
                        },
                    );
                }
            }
            Update::Broadcast(ServiceUpdate::Module {
                component,
                resources,
            }) => {
                let resource = resources.first().ok_or(Error::Invalid("empty module"))?;
                let retained_bytes = self.resource_bytes();
                let carousel = self
                    .carousels
                    .get_mut(&component.tag())
                    .ok_or(Error::Invalid("module before DII"))?;
                if carousel.info().download_id != resource.download_id {
                    return Err(Error::Invalid("module download identity differs from DII"));
                }
                let dependencies = dependencies(carousel.info(), resource.module_id)?;
                if dependencies[0].version != resource.module_version {
                    return Err(Error::Invalid("module version differs from DII"));
                }
                let bytes = update.resource_bytes();
                let replaced = carousel
                    .modules
                    .get(&resource.module_id)
                    .map_or(0, |module| module.bytes);
                if retained_bytes - replaced + bytes > self.budget {
                    return Err(Error::Capacity { limit: self.budget });
                }
                carousel.modules.insert(
                    resource.module_id,
                    Module {
                        update,
                        dependencies,
                        bytes,
                    },
                );
            }
            Update::Broadcast(ServiceUpdate::Event { .. }) => {} // A replay must not fire old events.
        }
        Ok(())
    }

    fn resource_bytes(&self) -> usize {
        self.carousels
            .values()
            .flat_map(|component| component.modules.values())
            .map(|module| module.bytes)
            .sum()
    }

    /// A frozen, ordered view; module bytes are shared, never copied for replay.
    /// All DII precede every module, so startup can resolve cross-component locks.
    pub fn replay(&self) -> Vec<Arc<Update>> {
        let mut result = vec![self.program.clone()];
        result.extend(
            self.time
                .iter()
                .chain(&self.clock)
                .chain(&self.broadcasters)
                .chain(&self.components)
                .cloned(),
        );
        result.extend(
            self.carousels
                .values()
                .map(|component| component.list.clone()),
        );
        result.extend(
            self.carousels
                .values()
                .flat_map(|component| component.modules.values())
                .map(|module| module.update.clone()),
        );
        result
    }
}

fn dependencies(info: &DownloadInfo, head: u16) -> Result<Vec<ModuleInfo>, Error> {
    let mut result: Vec<ModuleInfo> = Vec::new();
    let mut id = head;
    loop {
        if result.iter().any(|module| module.id == id) {
            return Err(Error::Invalid("module link cycle"));
        }
        let module = info
            .modules
            .iter()
            .find(|module| module.id == id)
            .ok_or(Error::Invalid("module missing from DII"))?;
        result.push(module.clone());
        match module.module_link()? {
            Some(ModuleLink::Head { next } | ModuleLink::Middle { next }) => id = next,
            Some(ModuleLink::Last) | None => return Ok(result),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arib_b24::{
        Descriptor,
        resource::{Resource, ResourceMapping},
        transport::{DataComponent, data_components_from_pmt},
    };

    fn component(tag: u8) -> DataComponent {
        let mut pmt = vec![
            0x02, 0xb0, 0x1d, 0, 42, 0xc1, 0, 0, 0xe1, 0, 0xf0, 0, 0x0d, 0xe1, 1, 0xf0, 0x0b, 0x52,
            1, tag, 0xfd, 6, 0, 0x0c, 0x33, 0x70, 0xf8, 0xa0,
        ];
        pmt.extend_from_slice(&viewer_mpegts::crc32_mpeg(&pmt).to_be_bytes());
        data_components_from_pmt(&pmt, 42).unwrap()[0]
    }
    fn info(version: u8, download: u32) -> DownloadInfo {
        DownloadInfo {
            transaction_id: 1,
            download_id: download,
            block_size: 1024,
            descriptors: vec![],
            modules: vec![ModuleInfo {
                id: 0,
                version,
                size: 3,
                descriptors: vec![],
            }],
        }
    }
    fn list(component: DataComponent, info: DownloadInfo) -> Arc<Update> {
        Arc::new(Update::Broadcast(ServiceUpdate::ModuleList {
            component,
            info,
        }))
    }
    fn module(component: DataComponent, version: u8, download_id: u32) -> Arc<Update> {
        Arc::new(Update::Broadcast(ServiceUpdate::Module {
            component,
            resources: vec![Resource {
                mapping: ResourceMapping::Entity,
                component_tag: Some(component.tag()),
                download_id,
                module_id: 0,
                module_version: version,
                module_name: vec![],
                name: b"a".to_vec(),
                media_type: Some(b"text/plain".to_vec()),
                data: vec![1, 2, 3],
            }],
        }))
    }
    fn snapshot() -> Snapshot {
        Snapshot::new(ServiceIdentity {
            service_id: 42,
            original_network_id: Some(1),
        })
    }
    fn modules(snapshot: &Snapshot) -> usize {
        snapshot
            .replay()
            .iter()
            .filter(|update| {
                matches!(
                    update.as_ref(),
                    Update::Broadcast(ServiceUpdate::Module { .. })
                )
            })
            .count()
    }

    #[test]
    fn replacements_removals_and_download_generations_invalidate_resources() {
        let mut state = snapshot();
        let component = component(0x40);
        state.apply(list(component, info(0, 1))).unwrap();
        let payload = module(component, 0, 1);
        state.apply(payload.clone()).unwrap();
        assert!(
            state
                .replay()
                .iter()
                .any(|item| Arc::ptr_eq(item, &payload))
        );
        state.apply(list(component, info(0, 1))).unwrap();
        assert_eq!(modules(&state), 1);
        state.apply(list(component, info(1, 1))).unwrap();
        assert_eq!(modules(&state), 0);
        state.apply(module(component, 1, 1)).unwrap();
        state.apply(list(component, info(1, 2))).unwrap();
        assert_eq!(modules(&state), 0);
        state.apply(module(component, 1, 2)).unwrap();
        let mut empty = info(1, 2);
        empty.modules.clear();
        state.apply(list(component, empty)).unwrap();
        assert_eq!(modules(&state), 0);
        state
            .apply(Arc::new(Update::Broadcast(ServiceUpdate::Components(
                vec![],
            ))))
            .unwrap();
        assert_eq!(state.replay().len(), 2); // Program and actual empty PMT only.
    }

    #[test]
    fn linked_tail_change_invalidates_the_joined_file() {
        let mut state = snapshot();
        let component = component(0x40);
        let mut linked = info(0, 1);
        linked.modules[0].descriptors.push(Descriptor {
            tag: 4,
            data: vec![0, 0, 1],
        });
        linked.modules.push(ModuleInfo {
            id: 1,
            version: 0,
            size: 1,
            descriptors: vec![Descriptor {
                tag: 4,
                data: vec![2, 0, 0],
            }],
        });
        state.apply(list(component, linked.clone())).unwrap();
        state.apply(module(component, 0, 1)).unwrap();
        assert_eq!(modules(&state), 1);
        linked.modules[1].version = 1;
        state.apply(list(component, linked)).unwrap();
        assert_eq!(modules(&state), 0);
    }

    #[test]
    fn all_lists_precede_resources_and_budget_failure_keeps_previous_snapshot() {
        let mut state = snapshot();
        for tag in [0x40, 0x50] {
            let component = component(tag);
            state.apply(list(component, info(0, 1))).unwrap();
            state.apply(module(component, 0, 1)).unwrap();
        }
        let replay = state.replay();
        assert!(replay[1..3].iter().all(|u| matches!(
            u.as_ref(),
            Update::Broadcast(ServiceUpdate::ModuleList { .. })
        )));
        let component = component(0x60);
        state.apply(list(component, info(0, 1))).unwrap();
        state.budget = state.resource_bytes();
        assert!(matches!(
            state.apply(module(component, 0, 1)),
            Err(Error::Capacity { .. })
        ));
        assert_eq!(modules(&state), 2);
    }
    #[test]
    fn replay_excludes_past_events_and_invalidated_broadcast_clocks() {
        let mut state = snapshot();
        let program = |ts| {
            Arc::new(Update::Metadata(metadata::Update::Program(
                viewer_web_bml::ProgramInfo {
                    service_id: 42,
                    original_network_id: Some(1),
                    transport_stream_id: ts,
                    event: None,
                },
            )))
        };
        state.apply(program(Some(1))).unwrap();
        state
            .apply(Arc::new(Update::Metadata(metadata::Update::Time(1000))))
            .unwrap();
        state
            .apply(Arc::new(Update::Metadata(metadata::Update::Clock {
                base: 90_000,
                extension: 0,
            })))
            .unwrap();
        state
            .apply(Arc::new(Update::Broadcast(ServiceUpdate::Event {
                component: component(0x40),
                section: arib_b24::event::EventSection {
                    data_event_id: 0,
                    group_id: 0,
                    version: 0,
                    section_number: 0,
                    last_section_number: 0,
                    messages: vec![arib_b24::event::EventMessage {
                        group_id: 0,
                        time: arib_b24::event::EventTime::Immediate,
                        message_type: 0,
                        message_id: 1,
                        private_data: vec![1],
                    }],
                    other_descriptors: vec![],
                },
            })))
            .unwrap();
        assert_eq!(state.replay().len(), 3);
        state.apply(program(None)).unwrap();
        assert_eq!(state.replay().len(), 1);
    }

    #[test]
    fn pending_writes_share_a_byte_budget_across_replaced_subscribers() {
        use super::super::{Client, QueueError, delivery::Delivery};
        use tokio::sync::{Semaphore, mpsc};
        let payload = module(component(0x40), 0, 1);
        let bytes = payload.resource_bytes();
        let budget = Arc::new(Semaphore::new(bytes));
        let (sender, mut receiver) = mpsc::channel(4);
        let old = Client {
            id: 1,
            sender,
            budget: budget.clone(),
        };
        let (sender, mut next_receiver) = mpsc::channel(4);
        let new = Client {
            id: 2,
            sender,
            budget: budget.clone(),
        };
        old.queue(Delivery::Snapshot {
            epoch: 1,
            updates: vec![payload.clone()],
        })
        .unwrap();
        drop(old);
        let writing = receiver.try_recv().unwrap();
        assert!(matches!(
            new.queue(Delivery::Update {
                epoch: 1,
                update: payload.clone()
            }),
            Err(QueueError::Budget(_))
        ));
        assert_eq!(budget.available_permits(), 0);
        drop(writing); // Completing or canceling the socket write returns its budget.
        new.queue(Delivery::Update {
            epoch: 1,
            update: payload,
        })
        .unwrap();
        drop(next_receiver.try_recv().unwrap());
        assert_eq!(budget.available_permits(), bytes);
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_list_payload_and_reset_sequences_never_replay_an_old_version(
            operations in proptest::collection::vec((0_u8..4, proptest::prelude::any::<u8>()), 0..80)
        ) {
            let mut state = snapshot();
            let component = component(0x40);
            let mut advertised = None;
            let mut retained = None;
            for (operation, version) in operations {
                match operation {
                    0 => { state = snapshot(); advertised = None; retained = None; },
                    1 => {
                        state.apply(list(component, info(version, 1))).unwrap();
                        if advertised != Some(version) { retained = None; }
                        advertised = Some(version);
                    },
                    2 => if let Some(version) = advertised {
                        state.apply(module(component, version, 1)).unwrap();
                        retained = Some(version);
                    },
                    _ => { /* Attaching a subscriber only reads the snapshot. */ },
                }
                let actual = state.replay().iter().filter_map(|update| match update.as_ref() {
                    Update::Broadcast(ServiceUpdate::Module { resources, .. }) => Some(resources[0].module_version),
                    _ => None,
                }).collect::<Vec<_>>();
                proptest::prop_assert_eq!(actual, retained.into_iter().collect::<Vec<_>>());
            }
        }
    }
}
