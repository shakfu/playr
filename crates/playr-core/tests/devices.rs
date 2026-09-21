//! Choosing an output device by ID. Lookups run against the machine's own
//! devices, so they assert only what holds with none.

use cpal::traits::DeviceTrait;
use playr_core::audio::output::{by_index, device, devices, OutputError};

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
    let listed = devices().unwrap();
    assert!(listed.iter().all(|d| !by_index(&d.id)));
    assert!(listed.iter().filter(|d| d.default).count() <= 1);
}
