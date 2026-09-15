//! One playr at a time.

use playr_app::instance::{claim_at, ALREADY_RUNNING};

#[test]
fn a_second_claim_is_refused_until_the_first_ends() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("playr/instance.lock");
    let first = claim_at(&path).unwrap();
    assert_eq!(claim_at(&path).unwrap_err(), ALREADY_RUNNING);
    drop(first);
    assert!(claim_at(&path).is_ok(), "the lock outlived its holder");
}
