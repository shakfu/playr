//! OSC: what received messages ask for, the listener over UDP, and the state
//! sent back.

#[path = "../../playr-core/tests/common/mod.rs"]
mod common;

use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::Duration;

use playr_app::action::Action;
use playr_app::config::Config;
use playr_app::model::Model;
use playr_core::audio::Mode;
use playr_core::db;
use playr_server::osc::{self, request, Feedback, Kind, RECEIVED, SENT};
use playr_server::owner::Request;
use rosc::{OscBundle, OscMessage, OscPacket, OscTime, OscType};

fn message(addr: &str, args: Vec<OscType>) -> OscMessage {
    OscMessage {
        addr: addr.into(),
        args,
    }
}

/// The action `addr` with `args` asks for, if it asks for one.
fn action(addr: &str, args: Vec<OscType>) -> Option<Action> {
    match request(&message(addr, args))? {
        Request::Perform(action) => Some(action),
        other => panic!("not an action: {other:?}"),
    }
}

#[test]
fn every_received_address_asks_for_something() {
    for address in RECEIVED {
        let args = match address.kind {
            Kind::Trigger => vec![],
            Kind::Float { max, .. } => vec![OscType::Float(max)],
            Kind::Int { min, .. } => vec![OscType::Int(min)],
            Kind::Text => vec![OscType::String("x".into())],
        };
        assert!(
            request(&message(address.path, args)).is_some(),
            "{}",
            address.path
        );
    }
    assert!(request(&message("/playr/quit", vec![])).is_none());
    assert!(request(&message("/playr/scan", vec![OscType::String("/".into())])).is_none());
}

#[test]
fn a_trigger_ignores_a_zero() {
    for args in [
        vec![],
        vec![OscType::Float(1.0)],
        vec![OscType::Int(1)],
        vec![OscType::Bool(true)],
    ] {
        assert_eq!(
            action("/playr/pause", args.clone()),
            Some(Action::TogglePause),
            "{args:?}"
        );
    }
    for args in [
        vec![OscType::Float(0.0)],
        vec![OscType::Int(0)],
        vec![OscType::Bool(false)],
    ] {
        assert_eq!(action("/playr/next", args.clone()), None, "{args:?}");
    }
}

#[test]
fn numbers_are_checked_against_their_range() {
    use OscType::{Float, Int, String};
    assert_eq!(
        action("/playr/volume", vec![Int(1)]),
        Some(Action::SetVolume(1.0))
    );
    assert_eq!(action("/playr/volume", vec![Float(1.5)]), None);
    assert_eq!(action("/playr/volume", vec![Float(f32::NAN)]), None);
    assert_eq!(action("/playr/volume", vec![]), None);
    assert_eq!(action("/playr/volume", vec![String("0.5".into())]), None);
    // A fader's float is rounded to a semitone.
    assert_eq!(
        action("/playr/speed", vec![Float(-2.6)]),
        Some(Action::SetSpeed(-3))
    );
    assert_eq!(action("/playr/speed", vec![Int(13)]), None);
    assert_eq!(
        action("/playr/mode", vec![Int(3)]),
        Some(Action::SetMode(Mode::RepeatOne))
    );
    assert_eq!(action("/playr/mode", vec![Float(1.5)]), None);
    assert_eq!(action("/playr/mode", vec![Int(4)]), None);

    let seek = |args| match request(&message("/playr/progress", args)) {
        Some(Request::Seek(f)) => Some(f),
        None => None,
        other => panic!("{other:?}"),
    };
    assert_eq!(seek(vec![Float(0.5)]), Some(0.5));
    assert_eq!(seek(vec![Float(-0.1)]), None);

    let playlist = |args| match request(&message("/playr/playlist", args)) {
        Some(Request::PlayPlaylistAt(i)) => Some(i),
        None => None,
        other => panic!("{other:?}"),
    };
    assert_eq!(playlist(vec![Int(2)]), Some(2));
    assert_eq!(playlist(vec![Float(2.0)]), Some(2));
    assert_eq!(playlist(vec![Int(-1)]), None);
}

#[test]
fn the_listener_opens_bundles_and_skips_what_it_cannot_read() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = socket.local_addr().unwrap();
    let (send, requests) = mpsc::channel();
    std::thread::spawn(move || osc::listen(socket, send));

    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client.send_to(b"not osc", addr).unwrap();
    let bundle = OscPacket::Bundle(OscBundle {
        timetag: OscTime::from((0, 1)),
        content: vec![
            OscPacket::Message(message("/playr/stop", vec![])),
            OscPacket::Message(message("/playr/unknown", vec![])),
            OscPacket::Message(message("/playr/speed", vec![OscType::Int(2)])),
        ],
    });
    client
        .send_to(&rosc::encoder::encode(&bundle).unwrap(), addr)
        .unwrap();

    let wait = Duration::from_secs(2);
    let mut got = Vec::new();
    for _ in 0..2 {
        match requests.recv_timeout(wait).unwrap() {
            Request::Perform(action) => got.push(action),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(got, [Action::Stop, Action::SetSpeed(2)]);
}

#[test]
fn the_schema_lists_both_tables() {
    let schema = osc::schema();
    let paths = |key: &str| -> Vec<String> {
        schema[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["address"].as_str().unwrap().to_string())
            .collect()
    };
    let received: Vec<&str> = RECEIVED.iter().map(|a| a.path).collect();
    let sent: Vec<&str> = SENT.iter().map(|a| a.path).collect();
    assert_eq!(paths("received"), received);
    assert_eq!(paths("sent"), sent);
    assert_eq!(schema["received"][4]["type"], "float");
    assert_eq!(schema["received"][4]["max"], 1.0);
    assert!(schema["received"][8].get("max").is_none());
}

fn model() -> Model {
    let conn = db::open_memory().unwrap();
    Model::new(conn, common::fake_player().0, Vec::new(), Config::default())
}

#[test]
fn the_values_sent_are_those_in_the_table() {
    let mut model = model();
    model.refresh();
    let values = osc::values(&model);
    let paths: Vec<&str> = values.iter().map(|(p, _)| *p).collect();
    let sent: Vec<&str> = SENT.iter().map(|a| a.path).collect();
    assert_eq!(paths, sent);
    assert!(values.contains(&("/playr/state", OscType::Int(0))));
    assert!(values.contains(&("/playr/time", OscType::String("0:00 / --".into()))));
}

/// The addresses of the messages waiting on `socket`.
fn received(socket: &UdpSocket) -> Vec<String> {
    let mut buf = [0u8; rosc::decoder::MTU];
    let mut addrs = Vec::new();
    while let Ok(n) = socket.recv(&mut buf) {
        match rosc::decoder::decode_udp(&buf[..n]).unwrap().1 {
            OscPacket::Message(m) => addrs.push(m.addr),
            OscPacket::Bundle(_) => panic!("a bundle"),
        }
    }
    addrs
}

#[test]
fn feedback_sends_everything_once_then_only_changes() {
    let tablet = UdpSocket::bind("127.0.0.1:0").unwrap();
    tablet
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut feedback = Feedback::new(tablet.local_addr().unwrap()).unwrap();
    let mut model = model();
    model.refresh();

    feedback.send(&model);
    assert_eq!(received(&tablet).len(), SENT.len());
    feedback.send(&model);
    assert!(received(&tablet).is_empty());

    model.perform(Action::SetVolume(0.5));
    model.refresh();
    feedback.send(&model);
    assert_eq!(received(&tablet), ["/playr/volume"]);
}
