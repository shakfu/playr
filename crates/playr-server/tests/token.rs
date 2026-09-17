//! The token file.

use playr_server::token::{load_or_create, matches};

#[test]
fn a_token_is_made_once_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("server.token");
    let made = load_or_create(&path).unwrap();
    assert_eq!(made.len(), 64);
    assert!(made.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(load_or_create(&path).unwrap(), made);

    let other = tempfile::tempdir().unwrap();
    assert_ne!(load_or_create(&other.path().join("t")).unwrap(), made);
}

#[cfg(unix)]
#[test]
fn only_its_owner_can_read_the_token() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.token");
    load_or_create(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_file_that_is_not_a_token_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.token");
    std::fs::write(&path, "hunter2").unwrap();
    assert!(load_or_create(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hunter2");
}

#[test]
fn tokens_match_only_when_equal() {
    assert!(matches("abcd", "abcd"));
    assert!(!matches("abcd", "abce"));
    assert!(!matches("abcd", "abc"));
    assert!(!matches("abcd", ""));
}
