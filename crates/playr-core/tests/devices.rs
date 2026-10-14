//! Choosing an output device by ID. Lookups run against the machine's own
//! devices, so they assert only what holds with none, and run only with
//! `PLAYR_DEVICE_TESTS` set; see `common::device_tests`.

mod common;

use cpal::traits::DeviceTrait;
use playr_core::audio::output::{
    by_index, device, devices, info, pipewire_sink, place, sink_description, DeviceInfo,
    OutputError,
};
use playr_core::audio::Player;

#[test]
fn alsa_card_indices_are_duplicates_and_names_are_not() {
    assert!(by_index("alsa:hw:CARD=2,DEV=0"));
    assert!(by_index("alsa:sysdefault:CARD=0"));
    assert!(!by_index("alsa:hw:CARD=Generic_1,DEV=0"));
    assert!(!by_index("alsa:hw:CARD=2x,DEV=0"));
    assert!(!by_index("alsa:default"));
    assert!(!by_index("alsa:CARD="));
}

#[test]
fn an_unknown_device_is_an_error_naming_it_and_the_command_that_lists_them() {
    if !common::device_tests() {
        return;
    }
    for want in ["alsa:no-such-pcm", "no-such-host:x", "no-such-pcm"] {
        let Err(e) = device(Some(want)) else {
            panic!("{want} was found");
        };
        assert!(
            matches!(&e, OutputError::NotFound { want: w, .. } if w == want),
            "{e:?}"
        );
        assert!(e.to_string().contains("`playr devices`"), "{e}");
    }
}

#[test]
fn every_listed_device_is_found_by_its_id_with_or_without_its_host() {
    if !common::device_tests() {
        return;
    }
    let host = cpal::default_host().id().to_string();
    for listed in devices().unwrap() {
        let found = device(Some(&listed.id)).unwrap();
        assert_eq!(found.id().unwrap().to_string(), listed.id);
        let bare = listed.id.strip_prefix(&format!("{host}:")).unwrap();
        assert_eq!(
            device(Some(bare)).unwrap().id().unwrap().to_string(),
            listed.id
        );
    }
}

#[test]
fn the_listing_holds_no_card_index_duplicates_and_one_default_at_most() {
    if !common::device_tests() {
        return;
    }
    let listed = devices().unwrap();
    assert!(listed.iter().all(|d| !by_index(&d.id)));
    assert!(listed.iter().filter(|d| d.default).count() <= 1);
}

#[test]
fn the_device_played_to_is_placed_in_the_listing_or_after_it() {
    let listed = |id: &str| DeviceInfo {
        id: id.into(),
        name: String::new(),
        default: false,
    };
    let mut all = vec![listed("alsa:default"), listed("alsa:pipewire")];
    assert_eq!(place(&mut all, listed("alsa:pipewire")), 1);
    assert_eq!(all.len(), 2);
    assert_eq!(place(&mut all, listed("alsa:hw:CARD=2,DEV=0")), 2);
    assert_eq!(all[2].id, "alsa:hw:CARD=2,DEV=0");
}

#[test]
fn the_player_reports_the_default_device_as_listed() {
    if !common::device_tests() {
        return;
    }
    let default = devices().unwrap().into_iter().find(|d| d.default);
    assert_eq!(info(&device(None).unwrap()), default);
    let player = Player::new(None).unwrap();
    assert_eq!(player.device(), default.as_ref());
}

#[test]
fn the_pipewire_sink_is_its_node_description() {
    let inspect = "id 33, type PipeWire:Interface:Node\n    \
        device.profile.description = \"Digital Stereo (HDMI 4)\"\n  \
        * node.description = \"Rembrandt Radeon (HDMI 4)\"\n  \
        * node.name = \"alsa_output.hdmi\"\n";
    assert_eq!(
        sink_description(inspect).as_deref(),
        Some("Rembrandt Radeon (HDMI 4)")
    );
    assert_eq!(sink_description("node.description = \"\"\n"), None);
    assert_eq!(sink_description(""), None);
}

#[test]
fn only_a_device_through_pipewire_asks_for_its_sink() {
    let listed = |id: &str, name: &str| DeviceInfo {
        id: id.into(),
        name: name.into(),
        default: false,
    };
    // Each returns before running `wpctl`.
    assert_eq!(
        pipewire_sink(&listed("alsa:hw:CARD=DAC,DEV=0", "DAC")),
        None
    );
    assert_eq!(
        pipewire_sink(&listed("alsa:default", "Default ALSA Output")),
        None
    );
}

#[test]
fn pipewire_names_the_sink_behind_its_alsa_device() {
    if !common::device_tests() {
        return;
    }
    let Some(pw) = devices()
        .unwrap()
        .into_iter()
        .find(|d| d.id == "alsa:pipewire")
    else {
        return;
    };
    assert!(pipewire_sink(&pw).is_some_and(|s| !s.is_empty()));
}
