//! Controller snapshot, glyph phase, and the only bytes that may go on the wire.
//! `toggle` is intentionally absent.

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub state: String,
    pub pipeline: String,
    pub error: Option<String>,
    #[allow(dead_code)]
    pub connected: bool,
    #[allow(dead_code)]
    pub engine: String,
    #[allow(dead_code)]
    pub usage: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Offline,
    Idle,
    IdleWarned,
    Starting,
    Recording,
    Connected,
    Transcribing,
    Delivering,
    Error,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    None,
    Start,
    Stop,
}

impl Phase {
    pub fn intent(self) -> Intent {
        match self {
            Phase::Idle | Phase::IdleWarned | Phase::Error => Intent::Start,
            Phase::Recording | Phase::Connected => Intent::Stop,
            _ => Intent::None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum LineEffect {
    Known(Snapshot),
    Unknown,
}

/// Subscription line, or a reply `result` object. Fail closed: anything that is
/// not a JSON object with a known `state` is [`LineEffect::Unknown`].
pub fn apply_line(line: &str) -> LineEffect {
    match serde_json::from_str::<Value>(line) {
        Ok(value) => apply_value(&value),
        Err(_) => LineEffect::Unknown,
    }
}

pub fn apply_value(value: &Value) -> LineEffect {
    if !value.is_object() {
        return LineEffect::Unknown;
    }
    let snap: Snapshot = match serde_json::from_value(value.clone()) {
        Ok(snap) => snap,
        Err(_) => return LineEffect::Unknown,
    };
    match snap.state.as_str() {
        "idle" | "starting" | "recording" | "connected" | "transcribing" | "delivering"
        | "error" => LineEffect::Known(snap),
        _ => LineEffect::Unknown,
    }
}

pub fn phase_of(snap: &Snapshot) -> Phase {
    match snap.state.as_str() {
        "idle" if snap.error.is_none() => Phase::Idle,
        "idle" => Phase::IdleWarned,
        "starting" => Phase::Starting,
        "recording" => Phase::Recording,
        "connected" => Phase::Connected,
        "transcribing" => Phase::Transcribing,
        "delivering" => Phase::Delivering,
        "error" => Phase::Error,
        _ => Phase::Unknown,
    }
}

#[derive(Clone, Debug)]
pub enum Reply {
    Applied(LineEffect),
    Rejected(String),
    Malformed,
}

pub fn parse_reply(line: &str) -> Reply {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Reply::Malformed;
    };
    if !value.is_object() {
        return Reply::Malformed;
    }
    match value.get("ok") {
        Some(Value::Bool(true)) => match value.get("result") {
            Some(result) if result.is_object() => Reply::Applied(apply_value(result)),
            _ => Reply::Malformed,
        },
        Some(Value::Bool(false)) => {
            let err = value
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("rejected");
            Reply::Rejected(err.to_string())
        }
        _ => Reply::Malformed,
    }
}

pub const SUBSCRIBE: &[u8] = b"{\"command\":\"subscribe\"}\n";
pub const START: &[u8] = b"{\"command\":\"start\"}\n";
pub const STOP: &[u8] = b"{\"command\":\"stop\"}\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Start,
    Stop,
}

impl Command {
    pub fn wire(self) -> &'static [u8] {
        match self {
            Command::Start => START,
            Command::Stop => STOP,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReqPhase {
    Connecting,
    Writing,
    Reading,
}

/// A new `start` only when nothing is in flight. A `stop` may proceed while a
/// `start` is awaiting its reply (start sound still holding the controller lock).
pub fn allow_start(start: bool, stop: bool) -> bool {
    !start && !stop
}

pub fn allow_stop(start_phase: Option<ReqPhase>, stop: bool) -> bool {
    if stop {
        return false;
    }
    match start_phase {
        None | Some(ReqPhase::Reading) => true,
        Some(ReqPhase::Connecting | ReqPhase::Writing) => false,
    }
}
