//! The latest state, which readers wait on.

use std::time::Duration;

use playr_server::state::Latest;

#[test]
fn readers_wait_for_a_change() {
    let latest = Latest::default();
    assert_eq!(latest.newer_than(0, Duration::from_millis(10)), None);
    latest.set("a".into());
    assert_eq!(latest.newer_than(0, Duration::ZERO), Some((1, "a".into())));
    // The same text again is not a change.
    latest.set("a".into());
    assert_eq!(latest.newer_than(1, Duration::from_millis(10)), None);
    latest.set("b".into());
    assert_eq!(latest.newer_than(1, Duration::ZERO), Some((2, "b".into())));
}
