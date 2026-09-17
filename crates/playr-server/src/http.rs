//! The web page's HTTP server, on std sockets, with request heads parsed by
//! `httparse`.
//!
//! Every request needs the token, from the cookie that `GET /?token=` sets,
//! unless the server is open. Open or not, the `Host` header must name this
//! machine, not a domain an attacker could point at it, and a POST's `Origin`
//! must match it. Every response closes its
//! connection, so no request waits behind another.

use std::io::{self, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use playr_app::action::Action;
use serde_json::{json, Value};

use crate::owner::{Query, Refused, Request};
use crate::state::Latest;
use crate::{token, web};

/// The page, with its script and styles inline.
const PAGE: &str = include_str!("page.html");

const COOKIE: &str = "playr_token";

/// The largest request head and body read; more is refused.
const HEAD_LIMIT: usize = 8 * 1024;
const BODY_LIMIT: usize = 4 * 1024;

/// Connections open at once, most of them event streams.
const CONNECTIONS: usize = 32;

/// How long a client may take to send a request, or to take a write.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Least time between two states sent to one page.
const EVENT_INTERVAL: Duration = Duration::from_millis(66);

/// How long a request waits for the owner to answer.
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// Longest silence on an event stream; a comment is sent after it, so a
/// client that has gone is found and its connection closed.
const KEEPALIVE: Duration = Duration::from_secs(15);

/// What the server checks requests against.
pub struct Config {
    /// The token every request needs, or `None` to need none, on a network
    /// whose every device is trusted.
    pub token: Option<String>,
    /// Host names accepted besides IP addresses, `localhost` and `.local`
    /// names, compared without case.
    pub hosts: Vec<String>,
    /// The directory the page may rescan; the page cannot name another.
    pub music: Option<PathBuf>,
}

/// What every connection's thread shares.
struct Context {
    config: Config,
    requests: Sender<Request>,
    latest: Arc<Latest>,
    open: AtomicUsize,
}

/// Serves `listener` until accepting fails, a thread per connection.
pub fn serve(
    listener: TcpListener,
    config: Config,
    requests: Sender<Request>,
    latest: Arc<Latest>,
) -> io::Result<()> {
    let context = Arc::new(Context {
        config,
        requests,
        latest,
        open: AtomicUsize::new(0),
    });
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if context.open.fetch_add(1, Ordering::SeqCst) >= CONNECTIONS {
            context.open.fetch_sub(1, Ordering::SeqCst);
            let _ = respond(&mut stream, 503, "text/plain", &[], b"too many connections");
            continue;
        }
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = handle(&mut stream, &context);
            context.open.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}

/// A request as read: method, path, query, headers with lowercase names, body.
struct Incoming {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Incoming {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The value of `name` in the `Cookie` header.
    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v)
    }

    /// The value of `name` in the query string, undecoded: tokens are hex.
    fn query_value(&self, name: &str) -> Option<&str> {
        self.query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v)
    }
}

/// Why a request could not be read, as the status to answer with.
enum Unreadable {
    Status(u16, &'static str),
    Io,
}

impl From<io::Error> for Unreadable {
    fn from(_: io::Error) -> Self {
        Unreadable::Io
    }
}

fn handle(stream: &mut TcpStream, context: &Context) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let request = match read(stream) {
        Ok(request) => request,
        Err(Unreadable::Status(status, why)) => {
            respond(stream, status, "text/plain", &[], why.as_bytes())?;
            return linger(stream);
        }
        Err(Unreadable::Io) => return Ok(()),
    };
    let text = |stream: &mut TcpStream, status, body: &str| {
        respond(stream, status, "text/plain", &[], body.as_bytes())
    };

    let Some(host) = request
        .header("host")
        .filter(|h| host_allowed(h, &context.config.hosts))
    else {
        return text(stream, 403, "unknown host; see --host");
    };
    let config = &context.config;
    if let Some(token) = &config.token {
        if request.method == "GET" && request.path == "/" {
            if let Some(given) = request.query_value("token") {
                if !token::matches(token, given) {
                    return text(stream, 401, "wrong token");
                }
                // The cookie carries the token from here, and the address bar drops it.
                let cookie = format!(
                    "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000"
                );
                return respond(
                    stream,
                    303,
                    "text/plain",
                    &[("Location", "/"), ("Set-Cookie", &cookie)],
                    b"",
                );
            }
        }
        if !request
            .cookie(COOKIE)
            .is_some_and(|given| token::matches(token, given))
        {
            return text(
                stream,
                401,
                "open the address playr-server printed at startup, with its token",
            );
        }
    }
    // Behind a proxy serving TLS, the page's origin is https.
    let same_site = |origin: &str| {
        origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            == Some(host)
    };
    if request.method == "POST" && request.header("origin").is_some_and(|o| !same_site(o)) {
        return text(stream, 403, "request from another site");
    }

    let Ok(body) = std::str::from_utf8(&request.body) else {
        return text(stream, 400, "not UTF-8");
    };
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => respond(
            stream,
            200,
            "text/html; charset=utf-8",
            &[("Content-Security-Policy", PAGE_POLICY)],
            PAGE.as_bytes(),
        ),
        ("GET", "/events") => events(stream, &context.latest),
        ("GET", "/config") => json(stream, &json!({ "rescan": config.music.is_some() })),
        ("POST", "/command") => {
            let line = body.trim().to_string();
            refusable(stream, context, |reply| Request::Command { line, reply })
        }
        ("POST", "/key") => {
            let name = body.trim().to_string();
            refusable(stream, context, |reply| Request::Key { name, reply })
        }
        ("POST", "/row") => {
            let parsed = serde_json::from_str::<Value>(body).ok().and_then(|v| {
                Some((
                    web::view_named(v["view"].as_str()?)?,
                    usize::try_from(v["row"].as_u64()?).ok()?,
                    v["key"].as_str()?.to_string(),
                    match &v["command"] {
                        Value::Null => None,
                        command => Some(command.as_str()?.to_string()),
                    },
                ))
            });
            let Some((view, row, key, command)) = parsed else {
                return text(
                    stream,
                    400,
                    r#"send {"view": name, "row": number, "key": text, "command": text or null}"#,
                );
            };
            refusable(stream, context, |reply| Request::Row {
                view,
                row,
                key,
                command,
                reply,
            })
        }
        ("POST", "/search") => {
            let parsed = serde_json::from_str::<Value>(body)
                .ok()
                .and_then(|v| Some((v["query"].as_str()?.to_string(), v["done"].as_bool()?)));
            let Some((query, done)) = parsed else {
                return text(
                    stream,
                    400,
                    r#"send {"query": text, "done": true or false}"#,
                );
            };
            perform(stream, context, Request::Search { query, done })
        }
        ("POST", "/answer") => match body.trim() {
            "yes" => perform(stream, context, Request::Answer(true)),
            "no" => perform(stream, context, Request::Answer(false)),
            _ => text(stream, 400, "send yes or no"),
        },
        ("POST", "/name") => perform(stream, context, Request::Name(body.trim().to_string())),
        ("POST", "/close") => perform(stream, context, Request::Close),
        ("POST", "/rescan") => match &config.music {
            Some(dir) => perform(stream, context, Request::Perform(Action::Scan(dir.clone()))),
            None => text(stream, 404, "playr-server was started without --music"),
        },
        ("GET", "/rows") => {
            let number = |name| {
                request
                    .query_value(name)
                    .and_then(|v| v.parse::<usize>().ok())
            };
            let view = request.query_value("view").and_then(web::view_named);
            let (Some(view), Some(start), Some(count)) = (view, number("start"), number("count"))
            else {
                return text(stream, 400, "send view, start and count");
            };
            query(stream, context, Query::Rows { view, start, count })
        }
        ("GET", "/keys") => query(stream, context, Query::Keys),
        ("GET", "/help") => match request.query_value("list") {
            Some("keys") => query(stream, context, Query::Help { commands: false }),
            Some("commands") => query(stream, context, Query::Help { commands: true }),
            _ => text(stream, 400, "send list=keys or list=commands"),
        },
        ("POST", "/complete") => query(stream, context, Query::Completions(body.to_string())),
        (
            _,
            "/" | "/events" | "/config" | "/command" | "/key" | "/row" | "/search" | "/answer"
            | "/name" | "/close" | "/rescan" | "/rows" | "/keys" | "/help" | "/complete",
        ) => text(stream, 405, "method not allowed"),
        _ => text(stream, 404, "not found"),
    }
}

/// Sends `request` to the owner, and answers 204 once it is queued.
fn perform(stream: &mut TcpStream, context: &Context, request: Request) -> io::Result<()> {
    match context.requests.send(request) {
        Ok(()) => respond(stream, 204, "text/plain", &[], b""),
        Err(_) => respond(stream, 503, "text/plain", &[], b"playr is stopping"),
    }
}

/// Sends the request `make` builds around a reply channel, and waits for the
/// owner's answer.
fn ask<T>(context: &Context, make: impl FnOnce(Sender<T>) -> Request) -> Option<T> {
    let (reply, answer) = mpsc::channel();
    context.requests.send(make(reply)).ok()?;
    answer.recv_timeout(ANSWER_WITHIN).ok()
}

/// Answers 204 once the owner has done the request, or its refusal.
fn refusable(
    stream: &mut TcpStream,
    context: &Context,
    make: impl FnOnce(Sender<Result<(), Refused>>) -> Request,
) -> io::Result<()> {
    match ask(context, make) {
        Some(Ok(())) => respond(stream, 204, "text/plain", &[], b""),
        Some(Err((status, why))) => respond(stream, status, "text/plain", &[], why.as_bytes()),
        None => respond(stream, 503, "text/plain", &[], b"playr did not answer"),
    }
}

/// Answers with what the owner reads.
fn query(stream: &mut TcpStream, context: &Context, query: Query) -> io::Result<()> {
    match ask(context, |reply| Request::Read { query, reply }) {
        Some(body) => json(stream, &body),
        None => respond(stream, 503, "text/plain", &[], b"playr did not answer"),
    }
}

fn json(stream: &mut TcpStream, body: &Value) -> io::Result<()> {
    respond(
        stream,
        200,
        "application/json",
        &[],
        body.to_string().as_bytes(),
    )
}

/// Inline script and styles only, fetches to this server, and no framing.
const PAGE_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; \
    style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; \
    form-action 'none'; frame-ancestors 'none'";

/// Whether `host`, a `Host` header, names this machine: an IP address,
/// `localhost`, a `.local` name, or one of `extra`. A domain an attacker owns
/// can resolve to this machine, but its name is not on the list.
pub fn host_allowed(host: &str, extra: &[String]) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => match host.rsplit_once(':') {
            Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
            _ => host,
        },
    };
    let name = name.to_ascii_lowercase();
    name.parse::<IpAddr>().is_ok()
        || name == "localhost"
        || (name.ends_with(".local") && name.len() > ".local".len())
        || extra.iter().any(|h| h.eq_ignore_ascii_case(&name))
}

fn read(stream: &mut TcpStream) -> Result<Incoming, Unreadable> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let started = Instant::now();
    let (head_len, mut request) = loop {
        // Each read has its own timeout; this bounds a head sent a byte at a time.
        if started.elapsed() > TIMEOUT {
            return Err(Unreadable::Io);
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(Unreadable::Io);
        }
        buf.extend_from_slice(&chunk[..n]);
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut parsed = httparse::Request::new(&mut headers);
        match parsed.parse(&buf) {
            Ok(httparse::Status::Complete(len)) => break (len, incoming(&parsed)),
            Ok(httparse::Status::Partial) if buf.len() < HEAD_LIMIT => continue,
            Ok(httparse::Status::Partial) => {
                return Err(Unreadable::Status(431, "request head too large"))
            }
            Err(_) => return Err(Unreadable::Status(400, "malformed request")),
        }
    };
    if request.header("transfer-encoding").is_some() {
        return Err(Unreadable::Status(411, "send a Content-Length"));
    }
    let length = match request.header("content-length") {
        None if request.method == "POST" => {
            return Err(Unreadable::Status(411, "send a Content-Length"))
        }
        None => 0,
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| Unreadable::Status(400, "bad Content-Length"))?,
    };
    if length > BODY_LIMIT {
        return Err(Unreadable::Status(413, "body too large"));
    }
    let mut body = buf.split_off(head_len.min(buf.len()));
    body.truncate(length);
    if body.len() < length {
        let start = body.len();
        body.resize(length, 0);
        stream.read_exact(&mut body[start..])?;
    }
    request.body = body;
    Ok(request)
}

fn incoming(parsed: &httparse::Request) -> Incoming {
    let target = parsed.path.unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    Incoming {
        method: parsed.method.unwrap_or_default().to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers: parsed
            .headers
            .iter()
            .map(|h| {
                (
                    h.name.to_ascii_lowercase(),
                    String::from_utf8_lossy(h.value).into_owned(),
                )
            })
            .collect(),
        body: Vec::new(),
    }
}

/// Closes after a request left unread. Closing with bytes still to read
/// resets the connection, and the client can lose the response sent before it.
fn linger(stream: &mut TcpStream) -> io::Result<()> {
    stream.shutdown(std::net::Shutdown::Write)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let _ = io::copy(&mut stream.take(64 * 1024), &mut io::sink());
    Ok(())
}

/// Sends each new state as a server-sent event until the client goes.
fn events(stream: &mut TcpStream, latest: &Latest) -> io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\n\
          Content-Type: text/event-stream\r\n\
          Cache-Control: no-store\r\n\
          Connection: close\r\n\r\n",
    )?;
    let mut seen = 0;
    loop {
        let sent = Instant::now();
        match latest.newer_than(seen, KEEPALIVE) {
            Some((number, text)) => {
                seen = number;
                write!(stream, "data: {text}\n\n")?;
            }
            None => stream.write_all(b": keepalive\n\n")?,
        }
        stream.flush()?;
        std::thread::sleep(EVENT_INTERVAL.saturating_sub(sent.elapsed()));
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         Connection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
