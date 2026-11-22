//! Hardware-free Player contract and model checks.
use super::ffi;
use crate::{hdmi_cec::DeviceInfo, settings};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QModelIndex, QString};
use std::sync::{Arc, Mutex};

pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.toml");
    let mut player = ffi::new_player();
    assert!(player.rust().media.playback().is_none());
    assert!(!player.hdmi_cec_enabled());
    assert!(
        !player.hdmi_cec_allowed(),
        "connection tests use --features=none"
    );
    assert_eq!(
        player.hdmi_cec_state().to_string(),
        if cfg!(target_os = "linux") {
            "disabled"
        } else {
            "unsupported"
        }
    );
    assert_eq!(player.hdmi_cec_osd_name().to_string(), "NagameTV");
    assert!(!player.pin_mut().set_hdmi_cec_enabled(true));
    player.pin_mut().rust_mut().preferences =
        settings::Loaded::open(path.clone())?.activate(None, None);
    let changes = Arc::new(Mutex::new(0));
    let observed = changes.clone();
    let _signal = player.pin_mut().on_hdmi_cec_changed(move |player| {
        assert_eq!(
            player.hdmi_cec_device().to_string(),
            player.rust().preferences.preferences().hdmi_cec_device
        );
        assert!(!player.hdmi_cec_enabled());
        assert_eq!(
            player.hdmi_cec_state().to_string(),
            if cfg!(target_os = "linux") {
                "disabled"
            } else {
                "unsupported"
            }
        );
        assert!(player.hdmi_cec_error().is_empty());
        assert!(!player.hdmi_cec_external_without_passthrough());
        assert!(player.hdmi_cec_physical_address().is_empty());
        assert_eq!(player.rust().hdmi_cec_model.count(), 0);
        *observed.lock().unwrap() += 1;
    });
    assert!(
        player
            .pin_mut()
            .select_hdmi_cec_device(QString::from("/dev/cec2"))
    );
    assert_eq!(
        settings::Loaded::open(path.clone())?
            .preferences()
            .hdmi_cec_device,
        "/dev/cec2"
    );
    assert!(
        !player
            .pin_mut()
            .select_hdmi_cec_device(QString::from("/tmp/not-cec"))
    );
    assert!(player.pin_mut().set_hdmi_cec_enabled(false));
    player.pin_mut().refresh_hdmi_cec_devices();
    assert_eq!(*changes.lock().unwrap(), 3);

    let mut model = crate::hdmi_cec_model::ffi::make_hdmi_cec_model();
    crate::hdmi_cec_model::ffi::check_hdmi_cec_model(model.pin_mut());
    let counts = Arc::new(Mutex::new(Vec::new()));
    let observed = counts.clone();
    let _model = model.pin_mut().on_changed(move |model| {
        observed.lock().unwrap().push(model.count());
        assert_eq!(model.count(), model.row_count(&QModelIndex::default()));
    });
    let rows: Arc<[DeviceInfo]> = vec![DeviceInfo {
        path: "/dev/cec1".into(),
        name: "HDMI adapter".into(),
        driver: "vc4_hdmi".into(),
        physical_address: None,
    }]
    .into();
    model.pin_mut().replace(rows.clone());
    let roles = model.role_names();
    for (name, value) in [
        ("path", "/dev/cec1"),
        ("name", "HDMI adapter"),
        ("driver", "vc4_hdmi"),
    ] {
        let role = roles
            .iter()
            .find(|(_, role)| role.to_string() == name)
            .map(|(id, _)| *id)
            .unwrap();
        assert_eq!(
            model
                .data(&model.model_index(0, 0, &QModelIndex::default()), role)
                .value::<QString>(),
            Some(QString::from(value))
        );
    }
    model.pin_mut().replace(rows);
    model.pin_mut().replace(Vec::new().into());
    assert_eq!(*counts.lock().unwrap(), [1, 0]);
    eprintln!("HDMI-CEC Player/model checks passed (fake data, no hardware)");
    Ok(())
}
