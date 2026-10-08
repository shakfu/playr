//! Marks written as an Audacity label track or a cue sheet.

use std::path::Path;
use std::time::Duration;

use playr_core::labels::{audacity, cue};

fn marks() -> Vec<(Duration, Option<String>)> {
    vec![
        (Duration::from_millis(1500), Some("break".into())),
        (Duration::from_millis(62_040), None),
        (Duration::from_secs(90), Some("solo\tout \"loud\"".into())),
    ]
}

#[test]
fn audacity_labels_are_points_a_line_each() {
    assert_eq!(
        audacity(&marks()),
        "1.500000\t1.500000\tbreak\n\
         62.040000\t62.040000\t\n\
         90.000000\t90.000000\tsolo out \"loud\"\n"
    );
    assert_eq!(audacity(&[]), "");
}

/// A track from the start comes first when no mark is at 0; times are in
/// cue frames of 1/75 s, and a mark without a label is titled by number.
#[test]
fn a_cue_sheet_starts_a_track_at_each_mark() {
    let sheet = cue(Path::new("/m/set \"live\".flac"), &marks());
    assert_eq!(
        sheet,
        "FILE \"/m/set 'live'.flac\" WAVE\n\
         \x20 TRACK 01 AUDIO\n    TITLE \"Track 01\"\n    INDEX 01 00:00:00\n\
         \x20 TRACK 02 AUDIO\n    TITLE \"break\"\n    INDEX 01 00:01:37\n\
         \x20 TRACK 03 AUDIO\n    TITLE \"Track 03\"\n    INDEX 01 01:02:03\n\
         \x20 TRACK 04 AUDIO\n    TITLE \"solo out 'loud'\"\n    INDEX 01 01:30:00\n"
    );
    let at_zero = cue(
        Path::new("a.mp3"),
        &[(Duration::ZERO, Some("intro".into()))],
    );
    assert_eq!(
        at_zero,
        "FILE \"a.mp3\" MP3\n  TRACK 01 AUDIO\n    TITLE \"intro\"\n    INDEX 01 00:00:00\n"
    );
}
