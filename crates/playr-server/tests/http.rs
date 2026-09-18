//! The HTTP server over a real socket. A fake owner answers each request and
//! passes the test a line describing it; what the owner does with a request
//! is tested in `owner.rs` and `web.rs`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use playr_server::http::{self, host_allowed};
use playr_server::owner::Request;
use playr_server::state::Latest;
use serde_json::{json, Value};

const TOKEN: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

struct Server {
    addr: SocketAddr,
    /// What the fake owner was sent, one line per request.
    seen: Receiver<String>,
    latest: Arc<Latest>,
}

fn server() -> Server {
    start(false, Some(TOKEN))
}

/// A server whose library has a recorded root when `rescan`, needing `token`
/// if one is given.
fn start(rescan: bool, token: Option<&str>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (send, requests) = mpsc::channel();
    let (tell, seen) = mpsc::channel();
    std::thread::spawn(move || fake_owner(requests, tell));
    let latest = Arc::new(Latest::default());
    let config = http::Config {
        token: token.map(String::from),
        hosts: vec!["pi.lan".into()],
        rescan,
    };
    let serving = latest.clone();
    std::thread::spawn(move || http::serve(listener, config, send, serving));
    Server { addr, seen, latest }
}

/// Describes each request to the test before answering it. A command or key
/// named `refuse` is refused with 403; a read answers with its description.
fn fake_owner(requests: Receiver<Request>, tell: mpsc::Sender<String>) {
    let refused = |what: &str| {
        if what == "refuse" {
            Err((403, "refused".to_string()))
        } else {
            Ok(())
        }
    };
    for request in requests {
        let line = match &request {
            Request::Perform(action) => format!("perform {action:?}"),
            Request::Command { line, .. } => format!("command {line}"),
            Request::Key { name, .. } => format!("key {name}"),
            Request::Row {
                view,
                row,
                key,
                command,
                ..
            } => format!("row {view:?} {row} {key} {command:?}"),
            Request::Search { query, done } => format!("search {query:?} {done}"),
            Request::Answer(yes) => format!("answer {yes}"),
            Request::Name(name) => format!("name {name}"),
            Request::Close => "close".into(),
            Request::Read { query, .. } => format!("read {query:?}"),
            Request::Seek(f) => format!("seek {f}"),
            Request::PlayPlaylistAt(i) => format!("playlist {i}"),
        };
        let _ = tell.send(line.clone());
        match request {
            Request::Command { line, reply } => {
                let _ = reply.send(refused(&line));
            }
            Request::Key { name, reply } => {
                let _ = reply.send(refused(&name));
            }
            Request::Row { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            Request::Read { reply, .. } => {
                let _ = reply.send(json!({ "read": line }));
            }
            _ => {}
        }
    }
}

impl Server {
    /// The requests the owner has been sent, up to a close sent now to mark
    /// the end, so a request refused before the owner is surely absent.
    fn requests(&self) -> Vec<String> {
        assert_eq!(self.send("POST", "/close", "").status, 204);
        let mut seen = Vec::new();
        loop {
            let line = self.seen.recv_timeout(Duration::from_secs(2)).unwrap();
            if line == "close" {
                return seen;
            }
            seen.push(line);
        }
    }

    /// Sends `head` and `body` as written, and reads the whole response.
    fn raw(&self, head: &str, body: &str) -> Response {
        let mut stream = TcpStream::connect(self.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .write_all(format!("{head}\r\n\r\n").as_bytes())
            .unwrap();
        stream.write_all(body.as_bytes()).unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        parse(&text)
    }

    /// A request from a signed-in browser at this server's address.
    fn send(&self, method: &str, path: &str, body: &str) -> Response {
        self.raw(
            &format!(
                "{method} {path} HTTP/1.1\r\nHost: {}\r\nCookie: playr_token={TOKEN}\r\n\
                 Content-Length: {}",
                self.addr,
                body.len()
            ),
            body,
        )
    }

    fn json(&self, method: &str, path: &str, body: &str) -> Value {
        let r = self.send(method, path, body);
        assert_eq!(r.status, 200, "{path}: {}", r.body);
        serde_json::from_str(&r.body).unwrap()
    }
}

/// A response's status, headers with lowercase names, and body.
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

fn parse(text: &str) -> Response {
    let (head, body) = text.split_once("\r\n\r\n").expect("a complete response");
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(": "))
        .map(|(n, v)| (n.to_ascii_lowercase(), v.to_string()))
        .collect();
    Response {
        status,
        headers,
        body: body.to_string(),
    }
}

#[test]
fn the_token_in_the_address_becomes_a_cookie() {
    let server = server();
    let host = server.addr;
    let r = server.raw(&format!("GET /?token={TOKEN} HTTP/1.1\r\nHost: {host}"), "");
    assert_eq!(r.status, 303);
    assert_eq!(r.header("location"), Some("/"));
    let cookie = r.header("set-cookie").unwrap();
    assert!(
        cookie.starts_with(&format!("playr_token={TOKEN};")),
        "{cookie}"
    );
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));

    let r = server.send("GET", "/", "");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("<title>playr</title>"));
    assert!(r.header("content-security-policy").is_some());
}

#[test]
fn every_route_needs_the_token() {
    let server = server();
    let host = server.addr;
    let wrong = "f".repeat(64);
    for head in [
        format!("GET / HTTP/1.1\r\nHost: {host}"),
        format!("GET /?token={wrong} HTTP/1.1\r\nHost: {host}"),
        format!("GET /events HTTP/1.1\r\nHost: {host}\r\nCookie: playr_token={wrong}"),
        format!("GET /rows?view=library&start=0&count=9 HTTP/1.1\r\nHost: {host}"),
        format!("POST /key HTTP/1.1\r\nHost: {host}\r\nContent-Length: 5"),
    ] {
        let body = if head.contains("Content-Length") {
            "space"
        } else {
            ""
        };
        let r = server.raw(&head, body);
        assert_eq!(r.status, 401, "{head}");
    }
    assert!(server.requests().is_empty());
}

#[test]
fn a_host_that_is_not_this_machine_is_refused() {
    let server = server();
    // A page on evil.example whose name now resolves to this machine.
    let r = server.raw(
        &format!(
            "POST /command HTTP/1.1\r\nHost: evil.example:8080\r\n\
             Cookie: playr_token={TOKEN}\r\nContent-Length: 5"
        ),
        "pause",
    );
    assert_eq!(r.status, 403);
    assert!(server.requests().is_empty());
}

#[test]
fn a_post_from_another_origin_is_refused() {
    let server = server();
    let host = server.addr;
    let post = |origin: &str| {
        server.raw(
            &format!(
                "POST /command HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\n\
                 Cookie: playr_token={TOKEN}\r\nContent-Length: 5"
            ),
            "pause",
        )
    };
    assert_eq!(post("http://evil.example").status, 403);
    assert_eq!(post(&format!("http://{host}")).status, 204);
    assert_eq!(server.requests(), ["command pause"]);
}

#[test]
fn a_proxy_serving_tls_gives_an_https_origin() {
    let server = server();
    let host = server.addr;
    let r = server.raw(
        &format!(
            "POST /command HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}\r\n\
             Cookie: playr_token={TOKEN}\r\nContent-Length: 5"
        ),
        "pause",
    );
    assert_eq!(r.status, 204);
    let r = server.raw(
        &format!(
            "POST /command HTTP/1.1\r\nHost: {host}\r\nOrigin: https://{host}.evil.example\r\n\
             Cookie: playr_token={TOKEN}\r\nContent-Length: 5"
        ),
        "pause",
    );
    assert_eq!(r.status, 403);
    assert_eq!(server.requests(), ["command pause"]);
}

#[test]
fn an_open_server_needs_no_token_but_still_checks_host_and_origin() {
    let server = start(false, None);
    let host = server.addr;
    let r = server.raw(&format!("GET / HTTP/1.1\r\nHost: {host}"), "");
    assert_eq!(r.status, 200);
    assert!(r.header("set-cookie").is_none());
    let post = |head: String| server.raw(&head, "pause").status;
    assert_eq!(
        post(format!(
            "POST /command HTTP/1.1\r\nHost: {host}\r\nContent-Length: 5"
        )),
        204
    );
    assert_eq!(
        post(format!(
            "POST /command HTTP/1.1\r\nHost: {host}\r\nOrigin: http://evil.example\r\nContent-Length: 5"
        )),
        403
    );
    assert_eq!(
        post("POST /command HTTP/1.1\r\nHost: evil.example\r\nContent-Length: 5".into()),
        403
    );
    assert_eq!(server.requests(), ["command pause"]);
}

#[test]
fn commands_and_keys_reach_the_owner_and_its_refusal_comes_back() {
    let server = server();
    assert_eq!(server.send("POST", "/command", " speed =-3 ").status, 204);
    assert_eq!(server.send("POST", "/key", "shift-right").status, 204);
    let r = server.send("POST", "/command", "refuse");
    assert_eq!((r.status, r.body.as_str()), (403, "refused"));
    assert_eq!(server.send("POST", "/key", "refuse").status, 403);
    assert_eq!(
        server.requests(),
        [
            "command speed =-3",
            "key shift-right",
            "command refuse",
            "key refuse"
        ]
    );
}

#[test]
fn a_row_names_its_view_row_key_and_command() {
    let server = server();
    let row = r#"{"view": "selection", "row": 3, "key": "/m/a.flac", "command": "move -1"}"#;
    assert_eq!(server.send("POST", "/row", row).status, 204);
    let click = r#"{"view": "playlists", "row": 0, "key": "7", "command": null}"#;
    assert_eq!(server.send("POST", "/row", click).status, 204);
    for bad in [
        "",
        r#"{"view": "sampler", "row": 0, "key": "x"}"#,
        r#"{"view": "library", "row": -1, "key": "x"}"#,
        r#"{"view": "library", "row": 0}"#,
        r#"{"view": "library", "row": 0, "key": "x", "command": 5}"#,
    ] {
        assert_eq!(server.send("POST", "/row", bad).status, 400, "{bad}");
    }
    assert_eq!(
        server.requests(),
        [
            r#"row Selection 3 /m/a.flac Some("move -1")"#,
            "row Playlists 0 7 None"
        ]
    );
}

#[test]
fn prompts_are_answered_named_and_searched() {
    let server = server();
    assert_eq!(server.send("POST", "/answer", "yes").status, 204);
    assert_eq!(server.send("POST", "/answer", "no").status, 204);
    assert_eq!(server.send("POST", "/answer", "maybe").status, 400);
    assert_eq!(server.send("POST", "/name", " late night ").status, 204);
    let typed = r#"{"query": "evans", "done": false}"#;
    assert_eq!(server.send("POST", "/search", typed).status, 204);
    assert_eq!(server.send("POST", "/search", "evans").status, 400);
    assert_eq!(
        server.requests(),
        [
            "answer true",
            "answer false",
            "name late night",
            r#"search "evans" false"#
        ]
    );
}

#[test]
fn reads_answer_with_what_the_owner_returns() {
    let server = server();
    let rows = server.json("GET", "/rows?view=playlists&start=200&count=50", "");
    assert_eq!(
        rows["read"],
        "read Rows { view: Playlists, start: 200, count: 50 }"
    );
    assert_eq!(server.json("GET", "/keys", "")["read"], "read Keys");
    assert_eq!(
        server.json("GET", "/help?list=commands", "")["read"],
        "read Help { commands: true }"
    );
    assert_eq!(
        server.json("POST", "/complete", "mode sh")["read"],
        r#"read Completions("mode sh")"#
    );
    assert_eq!(server.send("GET", "/rows?view=library", "").status, 400);
    assert_eq!(server.send("GET", "/help?list=sampler", "").status, 400);
}

#[test]
fn a_rescan_covers_the_recorded_roots_and_names_no_directory() {
    let server = start(true, Some(TOKEN));
    assert_eq!(server.json("GET", "/config", ""), json!({ "rescan": true }));
    // A body naming a directory is ignored: the page cannot choose one.
    assert_eq!(server.send("POST", "/rescan", "/etc").status, 204);
    assert_eq!(server.requests(), ["perform Rescan"]);

    let server = self::server();
    assert_eq!(
        server.json("GET", "/config", ""),
        json!({ "rescan": false })
    );
    assert_eq!(server.send("POST", "/rescan", "").status, 404);
    assert!(server.requests().is_empty());
}

#[test]
fn malformed_and_oversized_requests_are_refused() {
    let server = server();
    let host = server.addr;
    let cookie = format!("Cookie: playr_token={TOKEN}");
    let r = server.raw(
        &format!("POST /command HTTP/1.1\r\nHost: {host}\r\n{cookie}"),
        "",
    );
    assert_eq!(r.status, 411);
    let r = server.raw(
        &format!(
            "POST /command HTTP/1.1\r\nHost: {host}\r\n{cookie}\r\nTransfer-Encoding: chunked"
        ),
        "5\r\npause\r\n0\r\n\r\n",
    );
    assert_eq!(r.status, 411);
    let r = server.raw(
        &format!("POST /command HTTP/1.1\r\nHost: {host}\r\n{cookie}\r\nContent-Length: 5000"),
        "",
    );
    assert_eq!(r.status, 413);
    let long = "a".repeat(9000);
    let r = server.raw(
        &format!("GET / HTTP/1.1\r\nHost: {host}\r\nX-Long: {long}"),
        "",
    );
    assert_eq!(r.status, 431);
    assert_eq!(server.raw("NOT HTTP AT ALL\r\n", "").status, 400);
    let mut stream = TcpStream::connect(host).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let head =
        format!("POST /command HTTP/1.1\r\nHost: {host}\r\n{cookie}\r\nContent-Length: 1\r\n\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    // A lone continuation byte is not UTF-8.
    stream.write_all(&[0x80]).unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).unwrap();
    assert_eq!(parse(&text).status, 400);
    assert!(server.requests().is_empty());
}

#[test]
fn unknown_routes_and_methods() {
    let server = server();
    assert_eq!(server.send("GET", "/missing", "").status, 404);
    assert_eq!(server.send("GET", "/command", "").status, 405);
    assert_eq!(server.send("POST", "/", "").status, 405);
    assert_eq!(server.send("POST", "/rows", "").status, 405);
}

#[test]
fn events_stream_the_latest_state_and_its_changes() {
    let server = server();
    server.latest.set(r#"{"state":"stopped"}"#.into());
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "GET /events HTTP/1.1\r\nHost: {}\r\nCookie: playr_token={TOKEN}\r\n\r\n",
        server.addr
    )
    .unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line, "HTTP/1.1 200 OK\r\n");
    let mut data = Vec::new();
    while data.len() < 2 {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if let Some(text) = line.strip_prefix("data: ") {
            data.push(text.trim_end().to_string());
            server.latest.set(r#"{"state":"playing"}"#.into());
        } else if line.starts_with("Content-Type") {
            assert_eq!(line.trim_end(), "Content-Type: text/event-stream");
        }
    }
    assert_eq!(data, [r#"{"state":"stopped"}"#, r#"{"state":"playing"}"#]);
}

#[test]
fn hosts_that_name_this_machine() {
    let extra = vec!["pi.lan".to_string()];
    for host in [
        "192.168.1.20:8080",
        "127.0.0.1",
        "[::1]:8080",
        "localhost:8080",
        "raspberrypi.local:8080",
        "RaspberryPi.Local",
        "PI.lan:80",
    ] {
        assert!(host_allowed(host, &extra), "{host}");
    }
    for host in [
        "evil.example",
        "evil.example:8080",
        ".local",
        "local",
        "pi.lan.evil.example",
        "",
    ] {
        assert!(!host_allowed(host, &extra), "{host}");
    }
}
