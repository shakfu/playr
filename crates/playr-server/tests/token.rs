//! The token file.

use playr_server::token::{address, load_or_create, matches};

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

#[cfg(unix)]
#[test]
fn a_token_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.token");
    let made = load_or_create(&path).unwrap();
    for mode in [0o640, 0o604, 0o620] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        let err = load_or_create(&path).unwrap_err();
        assert!(err.to_string().contains("chmod 600"), "{err}");
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(load_or_create(&path).unwrap(), made);
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

#[test]
fn the_token_is_printed_to_a_terminal_and_kept_out_of_a_log() {
    let listen = "0.0.0.0:8080".parse().unwrap();
    let file = std::path::Path::new("/home/me/.local/share/playr/server.token");
    let token = "ab".repeat(32);
    let to_terminal = address(listen, Some(&token), true, file);
    assert_eq!(to_terminal, format!("http://0.0.0.0:8080/?token={token}"));
    let to_log = address(listen, Some(&token), false, file);
    assert!(!to_log.contains(&token), "{to_log}");
    assert!(to_log.contains("/home/me/.local/share/playr/server.token"));
    assert_eq!(
        address(listen, None, false, file),
        "http://0.0.0.0:8080/ (open: no token)"
    );
}
