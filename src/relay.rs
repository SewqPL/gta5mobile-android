// muguet - relay.rs
//
// Client for the Grand-V relay, the multiplayer path the web port uses. The
// wire format is JSON text frames, not this project's own packets, and it is
// deliberately tiny: a room, an access code, and a 20 Hz pose stream. Taken
// from the launcher that ships the relay server (src/relay.rs in
// bytarch/Grand-V-Android-Port) and from launcher_player_flow.js in the game
// data, which sends the same `check` handshake before the game starts.

use anyhow::{bail, Context, Result};
use serde_json::{Value, json};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{WebSocket, client_tls_with_config};

/// The rate the relay advertises in its welcome frame.
pub const HZ: u32 = 20;
/// Two peers per room, the relay's own cap.
pub const CAPACITY: u64 = 2;
const TIMEOUT: Duration = Duration::from_secs(8);

/// A remote player's pose. Field limits mirror the relay's own snapshot()
/// validation so a hostile or broken peer cannot feed us NaN into the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub seq: u32,
    pub model: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The relay accepted us and told us who is already here.
    Welcome { id: String, peers: Vec<String> },
    Joined(String),
    Left(String),
    Pose { id: String, pose: Pose },
    /// Answer to a `check`: how many of CAPACITY seats are taken.
    Checked { players: u64 },
    Failed(String),
}

/// The first frame on the wire. `check` only validates a room, `join` enters it.
pub fn hello(kind: &str, room: &str, key: &str, build: &str) -> Value {
    json!({ "type": kind, "protocol": 1, "key": key, "room": room, "build": build, "map": "gta5" })
}

pub fn pose_frame(seq: u32, model: u32, position: [f32; 3], rotation: [f32; 3]) -> Value {
    json!({ "type": "pose", "seq": seq, "model": model, "position": position, "rotation": rotation })
}

/// Room names are 1-32 characters of [A-Za-z0-9_-], and access codes 4-128
/// printable ASCII. Both rules come from the launcher dialogs, which reject
/// anything else before it reaches the relay.
pub fn valid_room(room: &str) -> bool {
    !room.is_empty()
        && room.len() <= 32
        && room.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn valid_key(key: &str) -> bool {
    (4..=128).contains(&key.len()) && key.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

pub fn valid_url(url: &str) -> bool {
    // ponytail: ws:// only. The relay ships plain on 8012, and a wss build needs
    // rustls wired in here; add it when a certificate is actually in play.
    url.starts_with("ws://") && !url.contains(' ') && url.split_once("://").is_some_and(|(_, r)| !r.is_empty())
}

/// How long a single socket read may block. Short, because poll() runs on the
/// game thread; the handshake loops below wait on a deadline instead.
const READ: Duration = Duration::from_millis(250);

/// Open the socket ourselves so the read timeout is set before anything wraps
/// it: MaybeTlsStream exposes no timeouts of its own and poll() must not block.
fn connect_relay(url: &str, timeout: Duration) -> Result<WebSocket<MaybeTlsStream<TcpStream>>> {
    let rest = url.trim_start_matches("ws://").trim_end_matches('/');
    let (host, port) = rest.split_once(':').map(|(h, p)| (h.to_string(), p.parse().unwrap_or(80))).unwrap_or((rest.to_string(), 80));
    let addr = (host.as_str(), port).to_socket_addrs().context("relay address")?.next().context("relay address")?;
    let tcp = TcpStream::connect_timeout(&addr, timeout).with_context(|| format!("relay {url}"))?;
    tcp.set_read_timeout(Some(READ)).ok();
    tcp.set_write_timeout(Some(timeout)).ok();
    let (socket, _) = client_tls_with_config(url.into_client_request()?, tcp, None, None).with_context(|| format!("relay {url}"))?;
    Ok(socket)
}

/// A read that simply timed out is not an error here: poses arrive at 20 Hz and
/// nothing at all is normal between them.
fn waiting(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

/// Turn one inbound text frame into an event, dropping anything malformed the
/// same way the relay drops malformed poses.
pub fn parse(text: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(text).ok()?;
    match v["type"].as_str()? {
        "welcome" => Some(Event::Welcome {
            id: v["id"].as_str()?.to_string(),
            peers: v["peers"].as_array()?.iter().filter_map(|p| p.as_str().map(str::to_string)).collect(),
        }),
        "joined" => Some(Event::Joined(v["id"].as_str()?.to_string())),
        "left" => Some(Event::Left(v["id"].as_str()?.to_string())),
        "checked" => Some(Event::Checked { players: v["players"].as_u64()? }),
        "pose" => {
            let pose = parse_pose(&v)?;
            Some(Event::Pose { id: v["id"].as_str()?.to_string(), pose })
        }
        "error" => Some(Event::Failed(v["message"].as_str().unwrap_or("relay rejected the request").to_string())),
        _ => None,
    }
}

fn parse_pose(v: &Value) -> Option<Pose> {
    let seq = v["seq"].as_u64()?;
    if seq > i32::MAX as u64 {
        return None;
    }
    let model = v["model"].as_u64()?;
    if model > u32::MAX as u64 {
        return None;
    }
    let vec = |field: &str, limit: f64| -> Option<[f32; 3]> {
        let a = v[field].as_array()?;
        if a.len() != 3 {
            return None;
        }
        let mut out = [0.0; 3];
        for (slot, value) in out.iter_mut().zip(a) {
            let n = value.as_f64()?;
            if !n.is_finite() || n.abs() > limit {
                return None;
            }
            *slot = n as f32;
        }
        Some(out)
    };
    Some(Pose { seq: seq as u32, model: model as u32, position: vec("position", 20000.0)?, rotation: vec("rotation", 360.0)? })
}

/// A live relay connection. Blocking on purpose: the game thread reads poses
/// every frame and this only needs one socket on its own thread.
pub struct Session {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    pub id: String,
    seq: u32,
}

impl Session {
    /// Ask the relay whether a room could take us, without entering it. Returns
    /// how many of CAPACITY seats are already taken.
    pub fn check(url: &str, room: &str, key: &str, build: &str) -> Result<u64> {
        let mut socket = connect_relay(url, TIMEOUT)?;
        socket
            .send(tungstenite::Message::Text(hello("check", room, key, build).to_string().into()))
            .context("relay check")?;
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            if std::time::Instant::now() > deadline {
                bail!("relay did not answer the room check in time");
            }
            match socket.read() {
                Ok(tungstenite::Message::Text(text)) => match parse(&text) {
                    Some(Event::Checked { players }) => return Ok(players),
                    Some(Event::Failed(why)) => bail!("relay refused the room: {why}"),
                    _ => continue,
                },
                Ok(tungstenite::Message::Close(frame)) => bail!("relay closed the check: {}", frame.map(|f| f.reason.to_string()).unwrap_or_default()),
                Ok(_) => continue,
                Err(ref e) if waiting(e) => continue,
                Err(e) => return Err(e).context("relay check"),
            }
        }
    }

    /// Enter a room. The relay answers with our peer id and the peers already in
    /// it, which is the only place our own identity comes from.
    pub fn join(url: &str, room: &str, key: &str, build: &str) -> Result<(Session, Vec<String>)> {
        let mut socket = connect_relay(url, TIMEOUT)?;
        socket
            .send(tungstenite::Message::Text(hello("join", room, key, build).to_string().into()))
            .context("relay join")?;
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            if std::time::Instant::now() > deadline {
                bail!("relay did not answer the join in time");
            }
            match socket.read() {
                Ok(tungstenite::Message::Text(text)) => match parse(&text) {
                    Some(Event::Welcome { id, peers }) => return Ok((Session { socket, id, seq: 0 }, peers)),
                    Some(Event::Failed(why)) => bail!("relay refused the room: {why}"),
                    _ => continue,
                },
                Ok(tungstenite::Message::Close(frame)) => bail!("relay closed the join: {}", frame.map(|f| f.reason.to_string()).unwrap_or_default()),
                Ok(_) => continue,
                Err(ref e) if waiting(e) => continue,
                Err(e) => return Err(e).context("relay join"),
            }
        }
    }

    /// Publish our pose. The relay rejects a sequence that does not move
    /// forward, so the counter only ever climbs.
    pub fn send_pose(&mut self, model: u32, position: [f32; 3], rotation: [f32; 3]) -> Result<()> {
        self.seq += 1;
        self.socket
            .send(tungstenite::Message::Text(pose_frame(self.seq, model, position, rotation).to_string().into()))
            .context("relay pose")
    }

    /// The next event if one is waiting, without blocking the game for long.
    /// A read timeout just means nothing arrived, which is normal at 20 Hz.
    pub fn poll(&mut self) -> Option<Event> {
        match self.socket.read() {
            Ok(tungstenite::Message::Text(text)) => parse(&text),
            Ok(tungstenite::Message::Ping(payload)) => {
                let _ = self.socket.send(tungstenite::Message::Pong(payload));
                None
            }
            Ok(_) => None,
            Err(ref e) if waiting(e) => None,
            Err(_) => None,
        }
    }

    pub fn close(&mut self) {
        let _ = self.socket.send(tungstenite::Message::Close(None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_join_frame_matches_what_the_relay_expects() {
        let frame = hello("join", "friends", "s3cret", "8b0b5899ed");
        assert_eq!(frame["type"], "join");
        assert_eq!(frame["protocol"], 1);
        assert_eq!(frame["map"], "gta5");
        assert_eq!(frame["room"], "friends");
        assert_eq!(frame["key"], "s3cret");
        assert_eq!(frame["build"], "8b0b5899ed");
    }

    #[test]
    fn room_and_key_rules_are_the_launcher_rules() {
        assert!(valid_room("friends"));
        assert!(valid_room(&"a".repeat(32)));
        assert!(!valid_room(""));
        assert!(!valid_room(&"a".repeat(33)));
        assert!(!valid_room("has space"));
        assert!(!valid_room("sl/ash"));
        assert!(valid_key("s3cret"));
        assert!(!valid_key("abc"));
        assert!(!valid_key(&"a".repeat(129)));
        assert!(!valid_key("tab\there"));
    }

    #[test]
    fn only_plain_websocket_urls_are_accepted() {
        assert!(valid_url("ws://192.168.1.10:8012/relay"));
        assert!(!valid_url("wss://play.example/relay"), "wss is not wired up yet");
        assert!(!valid_url("http://play.example/relay"));
        assert!(!valid_url("ws://"));
    }

    #[test]
    fn a_welcome_carries_our_id_and_the_others() {
        let event = parse(r#"{"type":"welcome","protocol":1,"id":"ab12","peers":["cd34"],"hz":20}"#).unwrap();
        assert_eq!(event, Event::Welcome { id: "ab12".into(), peers: vec!["cd34".into()] });
    }

    #[test]
    fn a_pose_round_trips_and_the_relay_stamps_the_sender() {
        let sent = pose_frame(7, 0xdead_beef, [1.5, -2.0, 3.25], [0.0, 90.0, 180.0]);
        let mut framed = sent.clone();
        framed["id"] = json!("peer1");
        let event = parse(&framed.to_string()).unwrap();
        assert_eq!(
            event,
            Event::Pose {
                id: "peer1".into(),
                pose: Pose { seq: 7, model: 0xdead_beef, position: [1.5, -2.0, 3.25], rotation: [0.0, 90.0, 180.0] }
            }
        );
    }

    #[test]
    fn poses_outside_the_relay_limits_are_dropped() {
        let bad = |seq: Value, position: Value| {
            json!({ "type": "pose", "id": "p", "seq": seq, "model": 1, "position": position, "rotation": [0, 0, 0] }).to_string()
        };
        // Past i32::MAX the relay refuses the frame, so we must not believe it.
        assert_eq!(parse(&bad(json!(2147483648_u64), json!([0.0, 0.0, 0.0]))), None);
        assert_eq!(parse(&bad(json!(1_u64), json!([30000.0, 0.0, 0.0]))), None);
        assert_eq!(parse(&bad(json!(1_u64), json!([0.0, 0.0]))), None);
        assert!(parse(&bad(json!(1_u64), json!([0.0, 0.0, 0.0]))).is_some());
    }

    #[test]
    fn nan_never_reaches_the_engine() {
        let frame = json!({ "type": "pose", "id": "p", "seq": 1, "model": 1, "position": [0.0, 0.0, 0.0], "rotation": ["NaN", 0.0, 0.0] }).to_string();
        assert_eq!(parse(&frame), None);
    }

    #[test]
    fn a_check_reports_the_seats_taken() {
        assert_eq!(parse(r#"{"type":"checked","players":1,"capacity":2}"#), Some(Event::Checked { players: 1 }));
        assert_eq!(CAPACITY, 2);
    }

    #[test]
    fn unknown_and_broken_frames_are_ignored() {
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"type":"whatever"}"#), None);
        assert_eq!(parse(r#"{"type":"left"}"#), None, "a leave without an id is unusable");
        assert_eq!(parse(r#"{"type":"error","message":"Room full"}"#), Some(Event::Failed("Room full".into())));
    }
}