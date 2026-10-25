use playr_app::config::ServerSettings;
use playr_server::flags::Flags;
use playr_server::osc;

#[test]
fn a_flag_wins_over_the_settings_and_either_over_the_default() {
    let none = ServerSettings::default();
    let file = ServerSettings {
        listen: Some("0.0.0.0:8080".parse().unwrap()),
        open: Some(true),
        hosts: vec!["pi.home".into()],
        osc: Some("0.0.0.0:9000".parse().unwrap()),
        osc_reply: Some("ipad.local:9001".into()),
    };
    let defaults = Flags::of(&none, &none);
    assert_eq!(defaults.listen, "127.0.0.1:8080".parse().unwrap());
    assert!(!defaults.open);
    assert_eq!(Flags::of(&none, &file).hosts, ["pi.home"]);
    assert_eq!(
        Flags::of(&none, &file).osc_reply.as_deref(),
        Some("ipad.local:9001")
    );

    let given = ServerSettings {
        listen: Some("127.0.0.1:9999".parse().unwrap()),
        hosts: vec!["other".into()],
        osc_reply: Some("10.0.0.2:9001".into()),
        ..Default::default()
    };
    let flags = Flags::of(&given, &file);
    assert_eq!(flags.listen, "127.0.0.1:9999".parse().unwrap());
    assert_eq!(flags.hosts, ["other"], "--host replaces the list");
    assert_eq!(flags.osc_reply.as_deref(), Some("10.0.0.2:9001"));
    assert!(flags.open, "not given, so the file's");
}

#[test]
fn a_reply_host_is_resolved_preferring_ipv4() {
    assert_eq!(
        osc::resolve("127.0.0.1:9001").unwrap(),
        "127.0.0.1:9001".parse().unwrap()
    );
    assert!(osc::resolve("localhost:9001").unwrap().is_ipv4());
    assert!(osc::resolve("no-such-host.invalid:9001").is_err());
}
