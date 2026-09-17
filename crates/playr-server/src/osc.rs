//! OSC: received addresses become requests, and playback state is sent back.
//!
//! OSC 1.0 has no authentication, so what it receives only controls playback
//! and plays playlists. The addresses are listed in [`RECEIVED`] and [`SENT`],
//! which `playr-server osc-schema` prints for layout generators.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use playr_app::action::Action;
use playr_app::message::fmt_time;
use playr_app::meter;
use playr_app::model::Model;
use playr_core::audio::{Mode, State};
use rosc::{OscMessage, OscPacket, OscType};
use serde_json::{json, Value};

use crate::owner::Request;
use crate::state;

/// What an address carries.
pub enum Kind {
    /// No argument, or one whose zero is ignored: a button's release.
    Trigger,
    Float {
        min: f32,
        max: f32,
    },
    Int {
        min: i32,
        max: Option<i32>,
    },
    Text,
}

/// An address and what it carries, for `osc-schema`.
pub struct Address {
    pub path: &'static str,
    pub kind: Kind,
    pub help: &'static str,
}

const fn address(path: &'static str, kind: Kind, help: &'static str) -> Address {
    Address { path, kind, help }
}

const UNIT: Kind = Kind::Float { min: 0.0, max: 1.0 };
const SPEED: Kind = Kind::Int {
    min: -12,
    max: Some(12),
};
const MODE: Kind = Kind::Int {
    min: 0,
    max: Some(3),
};

/// The addresses received. A number argument may be an int or a float.
pub const RECEIVED: &[Address] = &[
    address("/playr/pause", Kind::Trigger, "play or pause"),
    address("/playr/next", Kind::Trigger, "next track"),
    address("/playr/prev", Kind::Trigger, "previous track"),
    address("/playr/stop", Kind::Trigger, "stop"),
    address(
        "/playr/progress",
        UNIT,
        "seek to this fraction of the track",
    ),
    address("/playr/volume", UNIT, "set the volume"),
    address("/playr/speed", SPEED, "set varispeed in semitones"),
    address("/playr/mode", MODE, "normal, shuffle, repeat or repeat-one"),
    address(
        "/playr/playlist",
        Kind::Int { min: 0, max: None },
        "play the playlist at this index, oldest first",
    ),
];

/// The addresses sent to the reply address, each with one argument.
pub const SENT: &[Address] = &[
    address("/playr/title", Kind::Text, "title of the track playing"),
    address("/playr/artist", Kind::Text, "artist of the track playing"),
    address(
        "/playr/state",
        Kind::Int {
            min: 0,
            max: Some(2),
        },
        "stopped, playing or paused",
    ),
    address(
        "/playr/progress",
        UNIT,
        "position as a fraction of the track",
    ),
    address(
        "/playr/time",
        Kind::Text,
        "position and length, as 1:23 / 4:56",
    ),
    address("/playr/volume", UNIT, "volume"),
    address("/playr/speed", SPEED, "varispeed in semitones"),
    address("/playr/mode", MODE, "normal, shuffle, repeat or repeat-one"),
    address(
        "/playr/level",
        UNIT,
        "loudness on the meter's -40 to 0 dB scale",
    ),
];

/// Both tables as JSON.
pub fn schema() -> Value {
    let table = |addresses: &[Address]| -> Value {
        addresses
            .iter()
            .map(|a| {
                let mut entry = json!({ "address": a.path, "help": a.help });
                let (kind, min, max) = match a.kind {
                    Kind::Trigger => ("trigger", Value::Null, Value::Null),
                    Kind::Float { min, max } => ("float", json!(min), json!(max)),
                    Kind::Int { min, max } => ("int", json!(min), json!(max)),
                    Kind::Text => ("string", Value::Null, Value::Null),
                };
                entry["type"] = kind.into();
                if !min.is_null() {
                    entry["min"] = min;
                }
                if !max.is_null() {
                    entry["max"] = max;
                }
                entry
            })
            .collect()
    };
    json!({ "received": table(RECEIVED), "sent": table(SENT) })
}

/// The request `message` asks for. `None` for an address not in [`RECEIVED`],
/// a missing or out-of-range argument, or a trigger's zero.
pub fn request(message: &OscMessage) -> Option<Request> {
    let first = message.args.first();
    let number = || first.and_then(number);
    let perform = |action| Some(Request::Perform(action));
    // A button sends on press and on release; acting on both would undo the press.
    let pressed = || first.is_none_or(|a| number_or_bool(a) != Some(0.0));
    let index = || {
        number()
            .filter(|n| *n >= 0.0 && n.fract() == 0.0)
            .map(|n| n as usize)
    };
    match message.addr.as_str() {
        "/playr/pause" if pressed() => perform(Action::TogglePause),
        "/playr/next" if pressed() => perform(Action::Next),
        "/playr/prev" if pressed() => perform(Action::Prev),
        "/playr/stop" if pressed() => perform(Action::Stop),
        "/playr/progress" => number()
            .filter(|n| (0.0..=1.0).contains(n))
            .map(Request::Seek),
        "/playr/volume" => number()
            .filter(|n| (0.0..=1.0).contains(n))
            .and_then(|n| perform(Action::SetVolume(n as f32))),
        // Rounded, so a fader can send it.
        "/playr/speed" => number()
            .map(f64::round)
            .filter(|n| (-12.0..=12.0).contains(n))
            .and_then(|n| perform(Action::SetSpeed(n as i32))),
        "/playr/mode" => index()
            .and_then(|i| Mode::NAMES.get(i))
            .and_then(|(_, mode)| perform(Action::SetMode(*mode))),
        "/playr/playlist" => index().map(Request::PlayPlaylistAt),
        _ => None,
    }
}

fn number(arg: &OscType) -> Option<f64> {
    match *arg {
        OscType::Int(n) => Some(n.into()),
        OscType::Long(n) => Some(n as f64),
        OscType::Float(n) if n.is_finite() => Some(n.into()),
        OscType::Double(n) if n.is_finite() => Some(n),
        _ => None,
    }
}

fn number_or_bool(arg: &OscType) -> Option<f64> {
    match *arg {
        OscType::Bool(b) => Some(if b { 1.0 } else { 0.0 }),
        _ => number(arg),
    }
}

/// Receives on `socket` and sends each request to the owner, until the owner
/// has gone.
pub fn listen(socket: UdpSocket, requests: Sender<Request>) {
    let mut buf = [0u8; rosc::decoder::MTU];
    loop {
        // An error here is about one datagram, or on Windows about an earlier send.
        let Ok((n, _)) = socket.recv_from(&mut buf) else {
            continue;
        };
        let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..n]) else {
            continue;
        };
        let mut messages = Vec::new();
        flatten(packet, &mut messages);
        for request in messages.iter().filter_map(request) {
            if requests.send(request).is_err() {
                return;
            }
        }
    }
}

/// The messages in `packet`, bundles opened, in order.
fn flatten(packet: OscPacket, into: &mut Vec<OscMessage>) {
    match packet {
        OscPacket::Message(message) => into.push(message),
        OscPacket::Bundle(bundle) => {
            for packet in bundle.content {
                flatten(packet, into);
            }
        }
    }
}

/// How often every value is sent again, changed or not, so a layout opened
/// after a change still shows it.
pub const RESEND: Duration = Duration::from_secs(2);

/// Sends [`SENT`]'s values to one address.
pub struct Feedback {
    socket: UdpSocket,
    to: SocketAddr,
    sent: HashMap<&'static str, OscType>,
    since: Instant,
}

impl Feedback {
    pub fn new(to: SocketAddr) -> io::Result<Feedback> {
        let any: SocketAddr = if to.is_ipv4() {
            ([0, 0, 0, 0], 0).into()
        } else {
            (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
        };
        Ok(Feedback {
            socket: UdpSocket::bind(any)?,
            to,
            sent: HashMap::new(),
            since: Instant::now(),
        })
    }

    /// Sends each value that changed since it was last sent, or all of them
    /// once [`RESEND`] has passed. A send that fails is tried at the next.
    pub fn send(&mut self, model: &Model) {
        if self.since.elapsed() >= RESEND {
            self.sent.clear();
            self.since = Instant::now();
        }
        for (addr, value) in values(model) {
            if self.sent.get(addr) == Some(&value) {
                continue;
            }
            let message = OscPacket::Message(OscMessage {
                addr: addr.into(),
                args: vec![value.clone()],
            });
            let sent = rosc::encoder::encode(&message)
                .is_ok_and(|bytes| self.socket.send_to(&bytes, self.to).is_ok());
            if sent {
                self.sent.insert(addr, value);
            }
        }
    }
}

/// The value of each address in [`SENT`], in its order.
pub fn values(model: &Model) -> Vec<(&'static str, OscType)> {
    let snapshot = model.snapshot();
    let status = &snapshot.status;
    let progress = match status.duration {
        Some(d) if !d.is_zero() => (snapshot.position.as_secs_f64() / d.as_secs_f64()).min(1.0),
        _ => 0.0,
    };
    let time = format!(
        "{} / {}",
        fmt_time(snapshot.position),
        status.duration.map_or("--".into(), fmt_time)
    );
    let text = |s: Option<String>| OscType::String(s.unwrap_or_default());
    vec![
        ("/playr/title", text(state::title(model))),
        (
            "/playr/artist",
            text(state::playing(model).and_then(|t| t.artist.clone())),
        ),
        (
            "/playr/state",
            OscType::Int(match status.state {
                State::Stopped => 0,
                State::Playing => 1,
                State::Paused => 2,
            }),
        ),
        ("/playr/progress", OscType::Float(progress as f32)),
        ("/playr/time", OscType::String(time)),
        ("/playr/volume", OscType::Float(snapshot.volume)),
        ("/playr/speed", OscType::Int(status.semitones)),
        (
            "/playr/mode",
            OscType::Int(
                Mode::NAMES
                    .iter()
                    .position(|(_, m)| *m == status.mode)
                    .unwrap_or(0) as i32,
            ),
        ),
        (
            "/playr/level",
            OscType::Float(snapshot.loudness.map_or(0.0, meter::fraction)),
        ),
    ]
}
