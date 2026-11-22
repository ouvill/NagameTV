use super::{
    ownership::{Backend, Device, LogicalConfig},
    *,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

const HDMI_0: &str = "/dev/cec0";
const HDMI_1: &str = "/dev/cec1";
const ADAPTER: &str = "vc4-hdmi";
const TV_ADDRESS: u16 = 0x2000;

struct FakeAdapter {
    name: String,
    config: LogicalConfig,
    physical: Option<u16>,
    rc: bool,
    log_addrs: bool,
    denied: bool,
    fail_configure: bool,
    fail_clear: bool,
}

impl Default for FakeAdapter {
    fn default() -> Self {
        Self {
            name: ADAPTER.into(),
            config: LogicalConfig::Empty,
            physical: Some(TV_ADDRESS),
            rc: true,
            log_addrs: true,
            denied: false,
            fail_configure: false,
            fail_clear: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Write {
    Configure,
    Clear,
}

#[derive(Default)]
struct Bus {
    adapters: BTreeMap<String, FakeAdapter>,
    writes: Vec<(String, Write)>,
    scans: usize,
    open_devices: usize,
}

#[derive(Clone, Default)]
struct Fake(Rc<RefCell<Bus>>);

impl Fake {
    fn with_adapter() -> Self {
        let fake = Self::default();
        fake.0
            .borrow_mut()
            .adapters
            .insert(HDMI_0.into(), FakeAdapter::default());
        fake
    }

    fn modify(&self, path: &str, change: impl FnOnce(&mut FakeAdapter)) {
        change(self.0.borrow_mut().adapters.get_mut(path).unwrap());
    }

    fn assert_writes(&self, writes: &[(&str, Write)]) {
        let bus = self.0.borrow();
        assert_eq!(
            bus.writes
                .iter()
                .map(|(path, action)| (path.as_str(), *action))
                .collect::<Vec<_>>(),
            writes
        );
        assert_eq!(bus.open_devices, 0);
    }

    fn start(&self, runtime_dir: Option<&Path>, preference: bool, selection: &str) -> Controller {
        Controller::with_session(
            LaunchPlan::Preferences,
            preference,
            selection.into(),
            self.session(runtime_dir),
        )
    }

    fn session(&self, runtime_dir: Option<&Path>) -> Session {
        Session::new(self.clone(), runtime_dir.map(Path::to_path_buf))
    }
}

struct FakeDevice {
    bus: Fake,
    path: String,
    name: String,
}

impl Backend for Fake {
    fn enumerate(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        let mut bus = self.0.borrow_mut();
        bus.scans += 1;
        Ok(bus
            .adapters
            .iter()
            .map(|(path, adapter)| DeviceInfo {
                path: path.clone(),
                name: adapter.name.clone(),
                driver: "fake".into(),
                physical_address: adapter.physical,
            })
            .collect())
    }

    fn open(&mut self, path: &str) -> Result<Box<dyn Device>, Error> {
        let mut bus = self.0.borrow_mut();
        let adapter = bus.adapters.get(path).ok_or(Error::NoDevice)?;
        if adapter.denied {
            return Err(Error::Permission {
                operation: "open",
                path: path.into(),
                source: io::ErrorKind::PermissionDenied.into(),
            });
        }
        let name = adapter.name.clone();
        bus.open_devices += 1;
        Ok(Box::new(FakeDevice {
            bus: self.clone(),
            path: path.into(),
            name,
        }))
    }
}

impl FakeDevice {
    fn read<T>(&self, read: impl FnOnce(&FakeAdapter) -> T) -> T {
        read(&self.bus.0.borrow().adapters[&self.path])
    }

    fn write(
        &self,
        action: Write,
        config: LogicalConfig,
        fail: impl FnOnce(&FakeAdapter) -> bool,
    ) -> Result<(), Error> {
        let mut bus = self.bus.0.borrow_mut();
        let adapter = bus.adapters.get_mut(&self.path).unwrap();
        if fail(adapter) {
            return Err(Error::Io {
                operation: "write logical addresses",
                path: self.path.clone(),
                source: io::Error::from_raw_os_error(libc::EBUSY),
            });
        }
        adapter.config = config;
        bus.writes.push((self.path.clone(), action));
        Ok(())
    }
}

impl Drop for FakeDevice {
    fn drop(&mut self) {
        self.bus.0.borrow_mut().open_devices -= 1;
    }
}

impl Device for FakeDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn supports_logical_addresses(&self) -> bool {
        self.read(|a| a.log_addrs)
    }

    fn supports_remote_control(&self) -> bool {
        self.read(|a| a.rc)
    }

    fn physical_address(&self) -> Result<Option<u16>, Error> {
        Ok(self.read(|a| a.physical))
    }

    fn logical_config(&self) -> Result<LogicalConfig, Error> {
        Ok(self.read(|a| a.config))
    }

    fn configure_playback(&self) -> Result<(), Error> {
        self.write(Write::Configure, LogicalConfig::NagameTv, |a| {
            a.fail_configure
        })
    }

    fn clear_logical_addresses(&self) -> Result<(), Error> {
        self.write(Write::Clear, LogicalConfig::Empty, |a| a.fail_clear)
    }
}

fn marker(runtime: &Path) -> PathBuf {
    runtime.join("nagametv/hdmi-cec/cec0.owner")
}

fn crash_marker(runtime: &Path, adapter_name: &str) {
    let path = marker(runtime);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("{HDMI_0}\n{adapter_name}\n")).unwrap();
}

#[test]
fn configures_an_empty_adapter_without_a_tv_and_refreshes_without_writing() {
    let fake = Fake::with_adapter();
    fake.modify(HDMI_0, |a| a.physical = None);
    let runtime = tempfile::tempdir().unwrap();
    let mut controller = fake.start(Some(runtime.path()), true, HDMI_0);
    assert!(matches!(controller.snapshot().state, State::Configured));
    assert_eq!(controller.snapshot().physical_address, None);
    assert_eq!(
        fs::read_to_string(marker(runtime.path())).unwrap(),
        format!("{HDMI_0}\n{ADAPTER}\n")
    );
    fake.assert_writes(&[(HDMI_0, Write::Configure)]);

    fake.modify(HDMI_0, |a| a.physical = Some(TV_ADDRESS));
    controller.refresh();
    assert_eq!(controller.snapshot().physical_address, Some(TV_ADDRESS));
    fake.assert_writes(&[(HDMI_0, Write::Configure)]);

    drop(controller);
    assert!(!marker(runtime.path()).exists());
    fake.assert_writes(&[(HDMI_0, Write::Configure), (HDMI_0, Write::Clear)]);
}

#[test]
fn takes_over_a_crashed_owner_without_rewriting_the_kernel_configuration() {
    let fake = Fake::with_adapter();
    fake.modify(HDMI_0, |a| a.config = LogicalConfig::NagameTv);
    let runtime = tempfile::tempdir().unwrap();
    crash_marker(runtime.path(), ADAPTER);

    let controller = fake.start(Some(runtime.path()), true, "");
    assert!(matches!(controller.snapshot().state, State::Configured));
    fake.assert_writes(&[]);
    drop(controller);
    assert!(!marker(runtime.path()).exists());
    fake.assert_writes(&[(HDMI_0, Write::Clear)]);
}

#[test]
fn leaves_matching_os_configuration_and_other_external_configurations_untouched() {
    for (config, has_marker, without_passthrough) in [
        (LogicalConfig::NagameTv, false, false),
        (
            LogicalConfig::External {
                rc_passthrough: true,
            },
            true,
            false,
        ),
        (
            LogicalConfig::External {
                rc_passthrough: false,
            },
            true,
            true,
        ),
    ] {
        let fake = Fake::with_adapter();
        fake.modify(HDMI_0, |a| a.config = config);
        let runtime = tempfile::tempdir().unwrap();
        if has_marker {
            crash_marker(runtime.path(), ADAPTER);
        }
        let mut controller = fake.start(Some(runtime.path()), true, "");
        assert_eq!(controller.snapshot().state.as_str(), "external");
        assert_eq!(
            controller.snapshot().state.external_without_passthrough(),
            without_passthrough
        );
        controller.refresh();
        controller.enable(false);
        drop(controller);
        assert!(!marker(runtime.path()).exists());
        assert_eq!(fake.0.borrow().adapters[HDMI_0].config, config);
        fake.assert_writes(&[]);
    }
}

#[test]
fn releases_only_while_the_marker_adapter_name_and_configuration_match() {
    for changed in [
        "kernel configuration",
        "adapter name",
        "marker removed",
        "device unplugged",
    ] {
        let fake = Fake::with_adapter();
        let runtime = tempfile::tempdir().unwrap();
        let mut controller = fake.start(Some(runtime.path()), true, "");
        match changed {
            "kernel configuration" => fake.modify(HDMI_0, |a| {
                a.config = LogicalConfig::External {
                    rc_passthrough: true,
                }
            }),
            "adapter name" => fake.modify(HDMI_0, |a| a.name = "another adapter".into()),
            "marker removed" => fs::remove_file(marker(runtime.path())).unwrap(),
            "device unplugged" => {
                assert!(fake.0.borrow_mut().adapters.remove(HDMI_0).is_some());
            }
            _ => unreachable!(),
        }
        assert!(controller.enable(false));
        assert!(matches!(controller.snapshot().state, State::Disabled));
        assert!(!marker(runtime.path()).exists());
        fake.assert_writes(&[(HDMI_0, Write::Configure)]);
    }
}

#[test]
fn detects_a_replaced_configuration_on_settings_refresh() {
    let fake = Fake::with_adapter();
    let runtime = tempfile::tempdir().unwrap();
    let mut controller = fake.start(Some(runtime.path()), true, "");
    fake.modify(HDMI_0, |a| {
        a.config = LogicalConfig::External {
            rc_passthrough: false,
        }
    });
    controller.refresh();
    assert!(controller.snapshot().state.external_without_passthrough());
    assert!(!marker(runtime.path()).exists());
    drop(controller);
    fake.assert_writes(&[(HDMI_0, Write::Configure)]);
}

#[test]
fn a_second_process_cannot_take_over_a_live_owner() {
    let fake = Fake::with_adapter();
    let runtime = tempfile::tempdir().unwrap();
    let first = fake.start(Some(runtime.path()), true, "");
    let second = fake.start(Some(runtime.path()), true, "");
    assert_eq!(second.snapshot().state.as_str(), "external");
    drop(second);
    assert!(marker(runtime.path()).exists());
    fake.assert_writes(&[(HDMI_0, Write::Configure)]);
    drop(first);
    fake.assert_writes(&[(HDMI_0, Write::Configure), (HDMI_0, Write::Clear)]);
}

#[test]
fn configures_without_runtime_dir_but_never_clears_that_configuration() {
    let fake = Fake::with_adapter();
    let mut controller = fake.start(None, true, "");
    controller.refresh();
    assert!(matches!(controller.snapshot().state, State::Configured));
    controller.enable(false);
    controller.enable(true);
    assert_eq!(controller.snapshot().state.as_str(), "external");
    drop(controller);
    fake.assert_writes(&[(HDMI_0, Write::Configure)]);
}

#[test]
fn off_on_and_device_change_release_only_the_previous_adapter() {
    let fake = Fake::with_adapter();
    fake.0
        .borrow_mut()
        .adapters
        .insert(HDMI_1.into(), FakeAdapter::default());
    let runtime = tempfile::tempdir().unwrap();
    let mut controller = fake.start(Some(runtime.path()), false, "");
    controller.refresh();
    assert_eq!(fake.0.borrow().scans, 0);
    assert!(controller.select(HDMI_0.into()).is_ok());
    assert_eq!(fake.0.borrow().scans, 0);
    controller.enable(true);
    controller.enable(false);
    controller.enable(true);
    assert!(controller.select(HDMI_1.into()).is_ok());
    assert_eq!(controller.selection(), HDMI_1);
    assert!(matches!(controller.snapshot().state, State::Configured));
    drop(controller);
    fake.assert_writes(&[
        (HDMI_0, Write::Configure),
        (HDMI_0, Write::Clear),
        (HDMI_0, Write::Configure),
        (HDMI_0, Write::Clear),
        (HDMI_1, Write::Configure),
        (HDMI_1, Write::Clear),
    ]);
}

#[test]
fn normal_startup_restores_saved_device_and_restricted_startup_stays_off() {
    let fake = Fake::with_adapter();
    fake.0
        .borrow_mut()
        .adapters
        .insert(HDMI_1.into(), FakeAdapter::default());
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("settings.toml");
    let mut settings = crate::settings::Loaded::open(path.clone())
        .unwrap()
        .activate(None, None);
    settings.change(crate::settings::Change::HdmiCecEnabled(true));
    settings.change(crate::settings::Change::HdmiCecDevice(HDMI_1.into()));
    settings.flush().unwrap();
    let restored = crate::settings::Loaded::open(path)
        .unwrap()
        .activate(None, None);
    let preferences = restored.preferences();
    let denied = Controller::with_session(
        LaunchPlan::Restricted("none".parse().unwrap()),
        preferences.hdmi_cec_enabled,
        preferences.hdmi_cec_device.clone(),
        fake.session(Some(runtime.path())),
    );
    assert!(!denied.allowed());
    assert_eq!(fake.0.borrow().scans, 0);
    let restored = fake.start(
        Some(runtime.path()),
        preferences.hdmi_cec_enabled,
        &preferences.hdmi_cec_device,
    );
    assert!(restored.enabled());
    fake.assert_writes(&[(HDMI_1, Write::Configure)]);
}

#[test]
fn auto_prefers_a_valid_physical_address_and_keeps_the_registered_adapter() {
    let fake = Fake::with_adapter();
    fake.modify(HDMI_0, |a| a.physical = None);
    fake.0
        .borrow_mut()
        .adapters
        .insert(HDMI_1.into(), FakeAdapter::default());
    let runtime = tempfile::tempdir().unwrap();
    let mut controller = fake.start(Some(runtime.path()), true, "");
    fake.modify(HDMI_0, |a| a.physical = Some(TV_ADDRESS));
    fake.modify(HDMI_1, |a| a.physical = None);
    controller.refresh();
    assert_eq!(controller.snapshot().physical_address, None);
    fake.assert_writes(&[(HDMI_1, Write::Configure)]);
}

#[test]
fn reports_no_device_missing_capabilities_and_permission_denied() {
    let fake = Fake::default();
    let runtime = tempfile::tempdir().unwrap();
    let mut controller = fake.start(Some(runtime.path()), true, "");
    assert!(matches!(controller.snapshot().state, State::NoDevice));

    fake.0.borrow_mut().adapters.insert(
        HDMI_0.into(),
        FakeAdapter {
            rc: false,
            ..Default::default()
        },
    );
    controller.refresh();
    assert!(matches!(
        controller.snapshot().state,
        State::NoRemoteControl
    ));

    fake.modify(HDMI_0, |a| {
        a.rc = true;
        a.log_addrs = false;
    });
    controller.refresh();
    assert!(matches!(
        controller.snapshot().state,
        State::Failed(Error::NoLogicalAddresses(_))
    ));

    fake.modify(HDMI_0, |a| a.denied = true);
    controller.refresh();
    assert!(matches!(
        controller.snapshot().state,
        State::PermissionDenied(_)
    ));
    fake.assert_writes(&[]);
}

#[test]
fn failed_configure_does_not_leave_a_marker_and_failed_release_keeps_one_for_retry() {
    let fake = Fake::with_adapter();
    let runtime = tempfile::tempdir().unwrap();
    crash_marker(runtime.path(), ADAPTER);
    fake.modify(HDMI_0, |a| a.fail_configure = true);
    let mut controller = fake.start(Some(runtime.path()), true, "");
    assert_eq!(controller.snapshot().state.as_str(), "error");
    assert!(!marker(runtime.path()).exists());
    fake.assert_writes(&[]);

    fake.modify(HDMI_0, |a| {
        a.fail_configure = false;
        a.fail_clear = true;
    });
    controller.refresh();
    assert!(matches!(
        controller.select(HDMI_1.into()),
        Err(SelectionError::ReleaseFailed)
    ));
    assert_eq!(controller.selection(), "");
    assert_eq!(controller.snapshot().state.as_str(), "error");
    assert!(marker(runtime.path()).exists());
    fake.assert_writes(&[(HDMI_0, Write::Configure)]);

    fake.modify(HDMI_0, |a| a.fail_clear = false);
    controller.enable(false);
    assert!(matches!(controller.snapshot().state, State::Disabled));
    assert!(!marker(runtime.path()).exists());
    fake.assert_writes(&[(HDMI_0, Write::Configure), (HDMI_0, Write::Clear)]);
}
