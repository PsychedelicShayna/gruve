//! Black-box harness. Never touches the live controller socket.
//! Every block is spawned with `--runtime` pointed at a temp dir.
#![allow(dead_code)]

use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SCALE: f64 = 0.05;
/// 6 s hold × 0.05.
pub const HOLD: Duration = Duration::from_millis(300);
/// 500 ms debounce × 0.05.
pub const DEBOUNCE: Duration = Duration::from_millis(25);
/// 1.5 s reconnect × 0.05.
pub const RECONNECT: Duration = Duration::from_millis(75);
/// 2 s connect / first-snapshot deadline × 0.05.
pub const CONNECT: Duration = Duration::from_millis(100);
/// 5 s (the "click at 5 s" extension point) × 0.05.
pub const EXTEND_AT: Duration = Duration::from_millis(250);

pub const FG_DIM: &str = "#a89984";
pub const BLUE: &str = "#458588";
pub const YELLOW: &str = "#d79921";
pub const RED: &str = "#cc241d";
pub const ORANGE: &str = "#d65d0e";

pub const GLYPH_OFFLINE: &str = "◌";
pub const GLYPH_IDLE: &str = "◯";
pub const GLYPH_STARTING: &str = "◉";
pub const GLYPH_ACTIVE: &str = "●";
pub const GLYPH_UNKNOWN: &str = "?";

/// Hand count from the font table. Not `text::cells`.
/// Wide: ◯ ◉ ● → … = 2; everything else that sanitization can emit = 1.
pub fn hand_cells(text: &str) -> usize {
    text.chars()
        .map(|ch| match ch {
            '◯' | '◉' | '●' | '→' | '…' => 2,
            _ => 1,
        })
        .sum()
}

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch(label: &str) -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "gruve-block-{label}-{}-{n}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
    path
}

pub fn uid() -> u32 {
    let status = fs::read_to_string("/proc/self/status").expect("uid");
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            return rest.split_whitespace().next().unwrap().parse().unwrap();
        }
    }
    panic!("no Uid in /proc/self/status");
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub raw_line: String,
    pub value: Value,
    pub full_text: String,
    pub plain: String,
    pub instance: Option<String>,
    pub min_width: Option<String>,
}

impl Frame {
    pub fn has_key(&self, key: &str) -> bool {
        self.value.get(key).is_some()
    }

    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.value
            .as_object()
            .map(|o| {
                o.keys()
                    .filter(|k| k.starts_with(prefix))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub fn strip_pango(markup: &str) -> String {
    let mut out = String::new();
    let mut chars = markup.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            for d in chars.by_ref() {
                if d == '>' {
                    break;
                }
            }
        } else if ch == '&' {
            let mut ent = String::new();
            for d in chars.by_ref() {
                if d == ';' {
                    break;
                }
                ent.push(d);
            }
            out.push(match ent.as_str() {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    out.push('&');
                    out.push_str(&ent);
                    out.push(';');
                    continue;
                }
            });
        } else {
            out.push(ch);
        }
    }
    out
}

fn parse_frame(line: &str) -> Result<Frame, String> {
    let value: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let obj = value.as_object().ok_or_else(|| "stdout line is not a JSON object".to_string())?;
    let full_text = obj
        .get("full_text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let instance = match obj.get("instance") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
        None => None,
    };
    let min_width = match obj.get("min_width") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(other) => Some(other.to_string()),
        None => None,
    };
    Ok(Frame {
        raw_line: line.to_string(),
        plain: strip_pango(&full_text),
        full_text,
        value,
        instance,
        min_width,
    })
}

pub fn format_frames(frames: &[Frame]) -> String {
    if frames.is_empty() {
        return "(no frames)".to_string();
    }
    frames
        .iter()
        .enumerate()
        .map(|(i, f)| {
            format!(
                "  [{i}] plain={:?} instance={:?} min_width={:?} full_text={:?}\n      {}",
                f.plain,
                f.instance,
                f.min_width,
                f.full_text,
                f.raw_line
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn dump_fail(what: &str, expected: &str, actual: &str, frames: &[Frame]) -> ! {
    panic!(
        "{what}\nexpected: {expected}\nactual: {actual}\nframes:\n{}",
        format_frames(frames)
    );
}

#[derive(Clone, Debug)]
enum ReplyPolicy {
    /// Keep the request socket open and never write a reply.
    Withhold,
    /// Drop the request socket with no reply line.
    Close,
    Reply(String),
}

enum SubCmd {
    Line(String),
    Raw(Vec<u8>),
    Close,
}

struct StubInner {
    stop: bool,
    requests: Vec<String>,
    subscribes: Vec<String>,
    all_lines: Vec<String>,
    sub_txs: Vec<std::sync::mpsc::Sender<SubCmd>>,
    reply: ReplyPolicy,
    open_requests: usize,
}

pub struct Stub {
    state: Arc<Mutex<StubInner>>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
    sock: PathBuf,
}

impl Stub {
    pub fn bind(runtime: &Path) -> Self {
        fs::create_dir_all(runtime).unwrap();
        let sock = runtime.join("control.sock");
        let _ = fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap_or_else(|e| {
            panic!("bind {}: {e}", sock.display())
        });
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(StubInner {
            stop: false,
            requests: Vec::new(),
            subscribes: Vec::new(),
            all_lines: Vec::new(),
            sub_txs: Vec::new(),
            reply: ReplyPolicy::Reply(
                r#"{"ok":true,"result":{"state":"idle","pipeline":"idle","error":null}}"#.into(),
            ),
            open_requests: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let state_t = Arc::clone(&state);
        let stop_t = Arc::clone(&stop);
        let join = std::thread::spawn(move || accept_loop(listener, state_t, stop_t));
        Self {
            state,
            stop,
            join: Some(join),
            sock,
        }
    }

    pub fn set_withhold(&self) {
        self.state.lock().unwrap().reply = ReplyPolicy::Withhold;
    }

    pub fn set_close_without_reply(&self) {
        self.state.lock().unwrap().reply = ReplyPolicy::Close;
    }

    pub fn set_reply(&self, body: &str) {
        self.state.lock().unwrap().reply = ReplyPolicy::Reply(body.to_string());
    }

    pub fn subscribe_count(&self) -> usize {
        self.state.lock().unwrap().subscribes.len()
    }

    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    pub fn all_lines(&self) -> Vec<String> {
        self.state.lock().unwrap().all_lines.clone()
    }

    pub fn open_requests(&self) -> usize {
        self.state.lock().unwrap().open_requests
    }

    pub fn wait_subscribes(&self, n: usize, timeout: Duration) {
        let start = Instant::now();
        while self.subscribe_count() < n {
            if start.elapsed() > timeout {
                let lines = self.all_lines();
                panic!(
                    "timed out waiting for {n} subscribe connection(s); have {}\nlines: {lines:?}",
                    self.subscribe_count()
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn wait_requests(&self, n: usize, timeout: Duration) -> Vec<String> {
        let start = Instant::now();
        while self.requests().len() < n {
            if start.elapsed() > timeout {
                panic!(
                    "timed out waiting for {n} request(s); have {:?}",
                    self.requests()
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.requests()
    }

    pub fn push_line(&self, line: &str) {
        let mut st = self.state.lock().unwrap();
        st.sub_txs.retain(|tx| tx.send(SubCmd::Line(line.to_string())).is_ok());
    }

    pub fn push_raw(&self, bytes: &[u8]) {
        let mut st = self.state.lock().unwrap();
        let owned = bytes.to_vec();
        st.sub_txs.retain(|tx| tx.send(SubCmd::Raw(owned.clone())).is_ok());
    }

    pub fn close_subs(&self) {
        let mut st = self.state.lock().unwrap();
        for tx in st.sub_txs.drain(..) {
            let _ = tx.send(SubCmd::Close);
        }
    }

    pub fn command_names(&self) -> Vec<String> {
        self.all_lines().iter().map(|l| command_of(l)).collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut st) = self.state.lock() {
            st.stop = true;
            for tx in st.sub_txs.drain(..) {
                let _ = tx.send(SubCmd::Close);
            }
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        let _ = fs::remove_file(&self.sock);
    }
}

fn command_of(line: &str) -> String {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|v| v.get("command").and_then(|c| c.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| format!("<unparsed:{line}>"))
}

fn accept_loop(listener: UnixListener, state: Arc<Mutex<StubInner>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || handle_conn(stream, state, stop));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

fn read_line(stream: &mut UnixStream, stop: &AtomicBool) -> Option<String> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stop.load(Ordering::SeqCst) {
            return None;
        }
        match stream.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                buf.push(byte[0]);
                if buf.len() > 4096 {
                    return None;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                continue;
            }
            Err(_) => return None,
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn handle_conn(mut stream: UnixStream, state: Arc<Mutex<StubInner>>, stop: Arc<AtomicBool>) {
    let Some(line) = read_line(&mut stream, &stop) else {
        return;
    };
    let cmd = command_of(&line);
    if cmd == "subscribe" {
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut st = state.lock().unwrap();
            st.all_lines.push(line.clone());
            st.subscribes.push(line);
            st.sub_txs.push(tx);
        }
        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(SubCmd::Line(s)) => {
                    if write_all_ignore(&mut stream, s.as_bytes()).is_err()
                        || write_all_ignore(&mut stream, b"\n").is_err()
                    {
                        return;
                    }
                    let _ = stream.flush();
                }
                Ok(SubCmd::Raw(bytes)) => {
                    if write_all_ignore(&mut stream, &bytes).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                }
                Ok(SubCmd::Close) => return,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    } else {
        let policy = {
            let mut st = state.lock().unwrap();
            st.all_lines.push(line.clone());
            st.requests.push(line);
            st.open_requests += 1;
            st.reply.clone()
        };
        let mark_closed = || {
            let mut st = state.lock().unwrap();
            st.open_requests = st.open_requests.saturating_sub(1);
        };
        match policy {
            ReplyPolicy::Withhold => {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                let mut buf = [0u8; 64];
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            continue;
                        }
                        Err(_) => break,
                        Ok(_) => {}
                    }
                }
                mark_closed();
            }
            ReplyPolicy::Close => {
                drop(stream);
                mark_closed();
            }
            ReplyPolicy::Reply(body) => {
                let _ = write_all_ignore(&mut stream, body.as_bytes());
                let _ = write_all_ignore(&mut stream, b"\n");
                let _ = stream.flush();
                drop(stream);
                mark_closed();
            }
        }
    }
}

fn write_all_ignore(stream: &mut UnixStream, bytes: &[u8]) -> std::io::Result<()> {
    stream.write_all(bytes)
}

pub struct World {
    pub home: PathBuf,
    pub runtime: PathBuf,
    pub config: PathBuf,
    pub launcher_log: PathBuf,
    pub pause_file: PathBuf,
    pub stub: Option<Stub>,
    child: Mutex<Child>,
    stdin: Option<ChildStdin>,
    frames: Arc<Mutex<Vec<Frame>>>,
    bad_lines: Arc<Mutex<Vec<String>>>,
    stderr: Arc<Mutex<String>>,
    _scratch: Vec<PathBuf>,
}

pub struct WorldOpts {
    pub with_stub: bool,
    pub timescale: bool,
    pub extra_env: Vec<(String, String)>,
    pub clear_xdg: bool,
    pub args: Vec<String>,
}

impl Default for WorldOpts {
    fn default() -> Self {
        Self {
            with_stub: true,
            timescale: true,
            extra_env: Vec::new(),
            clear_xdg: true,
            args: Vec::new(),
        }
    }
}

impl World {
    pub fn start() -> Self {
        Self::start_with(WorldOpts::default())
    }

    pub fn start_with(opts: WorldOpts) -> Self {
        let home = scratch("home");
        let runtime = scratch("runtime");
        let config = scratch("config");
        let launcher_dir = scratch("launcher");
        let launcher_log = launcher_dir.join("argv.log");
        let pause_file = scratch("pause").join("paused");
        fs::create_dir_all(config.join("presets")).unwrap();
        fs::create_dir_all(config.join("input-methods")).unwrap();
        let launcher = launcher_dir.join("bnuuy-stt");
        write_launcher(&launcher);

        let stub = if opts.with_stub {
            Some(Stub::bind(&runtime))
        } else {
            None
        };

        let bin = env!("CARGO_BIN_EXE_gruve-block");
        let mut cmd = Command::new(bin);
        cmd.env_clear();
        cmd.env("HOME", &home);
        cmd.env("PATH", std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()));
        cmd.env("LANG", "C");
        cmd.env("USER", "gruve-test");
        cmd.env("GRUVE_TEST_LAUNCHER_LOG", &launcher_log);
        if opts.timescale {
            cmd.env("GRUVE_BLOCK_TIMESCALE", "0.05");
        }
        // Safety: never inherit the live XDG runtime. Callers that must observe
        // the unset-XDG default (A12 print-paths) pass clear_xdg and no --runtime.
        if !opts.clear_xdg {
            if let Ok(v) = std::env::var("XDG_RUNTIME_DIR") {
                cmd.env("XDG_RUNTIME_DIR", v);
            }
            if let Ok(v) = std::env::var("XDG_CONFIG_HOME") {
                cmd.env("XDG_CONFIG_HOME", v);
            }
        }
        for (k, v) in &opts.extra_env {
            cmd.env(k, v);
        }
        let mut args = opts.args;
        if opts.with_stub || !args.iter().any(|a| a == "--print-paths") {
            if !args.iter().any(|a| a == "--runtime") && opts.with_stub {
                args.push("--runtime".into());
                args.push(runtime.display().to_string());
            }
            if !args.iter().any(|a| a == "--config-dir") && !args.iter().any(|a| a == "--print-paths")
            {
                args.push("--config-dir".into());
                args.push(config.display().to_string());
            }
            if !args.iter().any(|a| a == "--launcher") && !args.iter().any(|a| a == "--print-paths")
            {
                args.push("--launcher".into());
                args.push(launcher.display().to_string());
            }
        }
        cmd.args(&args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap_or_else(|e| panic!("spawn {bin}: {e}"));
        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let bad_lines = Arc::new(Mutex::new(Vec::new()));
        let err_buf = Arc::new(Mutex::new(String::new()));
        let frames_t = Arc::clone(&frames);
        let bad_t = Arc::clone(&bad_lines);
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                match stdout.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        if byte[0] == b'\n' {
                            let line = String::from_utf8_lossy(&buf).trim().to_string();
                            buf.clear();
                            if line.is_empty() {
                                continue;
                            }
                            match parse_frame(&line) {
                                Ok(frame) => frames_t.lock().unwrap().push(frame),
                                Err(e) => bad_t.lock().unwrap().push(format!("{e}: {line}")),
                            }
                        } else {
                            buf.push(byte[0]);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let err_t = Arc::clone(&err_buf);
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            *err_t.lock().unwrap() = s;
        });

        let pause_parent = pause_file.parent().unwrap().to_path_buf();
        Self {
            home: home.clone(),
            runtime: runtime.clone(),
            config: config.clone(),
            launcher_log,
            pause_file: pause_file.clone(),
            stub,
            child: Mutex::new(child),
            stdin,
            frames,
            bad_lines,
            stderr: err_buf,
            _scratch: vec![home, runtime, config, launcher_dir, pause_parent],
        }
    }

    pub fn stub(&self) -> &Stub {
        self.stub.as_ref().expect("scenario has no stub")
    }

    pub fn frames(&self) -> Vec<Frame> {
        self.frames.lock().unwrap().clone()
    }

    pub fn latest(&self) -> Frame {
        self.frames()
            .last()
            .cloned()
            .unwrap_or_else(|| dump_fail("no frame yet", "a frame", "none", &[]))
    }

    pub fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    pub fn bad_lines(&self) -> Vec<String> {
        self.bad_lines.lock().unwrap().clone()
    }

    pub fn wait_frame(&self, timeout: Duration, mut pred: impl FnMut(&Frame) -> bool) -> Frame {
        let start = Instant::now();
        let mut seen = 0usize;
        loop {
            let frames = self.frames();
            if let Some(f) = frames.iter().skip(seen).find(|f| pred(f)) {
                return f.clone();
            }
            seen = frames.len();
            if let Some(status) = self.child.lock().unwrap().try_wait().ok().flatten() {
                dump_fail(
                    "block exited while waiting for a frame",
                    "running",
                    &format!("{status}; stderr={}", self.stderr()),
                    &frames,
                );
            }
            if start.elapsed() > timeout {
                dump_fail(
                    "timed out waiting for frame",
                    "matching frame",
                    &format!("stderr={}", self.stderr()),
                    &self.frames(),
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn wait_plain(&self, plain: &str, timeout: Duration) -> Frame {
        let want = plain.to_string();
        self.wait_frame(timeout, move |f| f.plain == want)
    }

    pub fn wait_plain_contains(&self, needle: &str, timeout: Duration) -> Frame {
        let needle = needle.to_string();
        self.wait_frame(timeout, move |f| f.plain.contains(&needle))
    }

    /// Click using the latest frame's instance, unless `instance` is `Some`.
    /// `None` in the Option-option sense: pass `UseInstance::Latest` / `Omit` / `Stale`.
    pub fn click(&mut self, button: i32, relative_x: i32, width: i32, instance: ClickInstance) {
        let inst = match instance {
            ClickInstance::Latest => self.latest().instance,
            ClickInstance::Value(v) => Some(v),
            ClickInstance::Omit => None,
        };
        let line = click_line(button, relative_x, width, inst.as_deref());
        let stdin = self.stdin.as_mut().expect("stdin closed");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    pub fn click_latest(&mut self, button: i32, relative_x: i32, width: i32) {
        self.click(button, relative_x, width, ClickInstance::Latest);
    }

    pub fn close_stdin(&mut self) {
        self.stdin.take();
    }

    pub fn wait_exit(&mut self, timeout: Duration) -> ExitStatus {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.lock().unwrap().try_wait().unwrap() {
                return status;
            }
            if start.elapsed() > timeout {
                panic!("block did not exit; stderr={}", self.stderr());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.lock().unwrap().id()
    }

    pub fn assert_no_bad_lines(&self) {
        let bad = self.bad_lines();
        if !bad.is_empty() {
            dump_fail(
                "non-JSON stdout",
                "only i3bar JSON lines",
                &bad.join(" | "),
                &self.frames(),
            );
        }
    }

    pub fn assert_wire(&self) {
        let lines = self.stub().all_lines();
        for line in &lines {
            if line.contains("toggle") {
                panic!("toggle appeared on the wire: {line}");
            }
            let cmd = command_of(line);
            if cmd != "subscribe" && cmd != "start" && cmd != "stop" {
                panic!("illegal command {cmd} in {line}");
            }
        }
    }

    pub fn assert_frame_contract(&self, frame: &Frame, expanded: bool) {
        if frame.has_key("name") {
            dump_fail("frame carries name", "absent", "present", &[frame.clone()]);
        }
        let borders = frame.keys_with_prefix("border");
        if !borders.is_empty() {
            dump_fail(
                "frame carries border*",
                "absent",
                &borders.join(","),
                &[frame.clone()],
            );
        }
        let markup = frame.value.get("markup").and_then(|v| v.as_str()).unwrap_or("");
        if markup != "pango" {
            dump_fail("markup", "pango", markup, &[frame.clone()]);
        }
        if expanded {
            if frame.min_width.is_some() {
                dump_fail(
                    "expanded frame carries min_width",
                    "absent",
                    frame.min_width.as_deref().unwrap_or(""),
                    &[frame.clone()],
                );
            }
        } else if frame.min_width.as_deref() != Some("◯") {
            dump_fail(
                "collapsed min_width",
                "◯",
                &frame.min_width.clone().unwrap_or_else(|| "<absent>".into()),
                &[frame.clone()],
            );
        }
    }

    pub fn launcher_records(&self) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(&self.launcher_log) else {
            return Vec::new();
        };
        let mut records = Vec::new();
        let mut cur = Vec::new();
        for line in text.lines() {
            if let Some(arg) = line.strip_prefix("ARG ") {
                cur.push(arg.to_string());
            } else if line == "END" {
                records.push(std::mem::take(&mut cur));
            }
        }
        records
    }

    pub fn wait_launcher(&self, timeout: Duration) -> Vec<Vec<String>> {
        let start = Instant::now();
        loop {
            let rec = self.launcher_records();
            if !rec.is_empty() {
                return rec;
            }
            if start.elapsed() > timeout {
                panic!("launcher was not invoked; log missing at {}", self.launcher_log.display());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn frame_len(&self) -> usize {
        self.frames.lock().unwrap().len()
    }

    /// Wait for a frame that arrives after `after` (a previous `frame_len`).
    pub fn wait_after(
        &self,
        after: usize,
        timeout: Duration,
        mut pred: impl FnMut(&Frame) -> bool,
    ) -> Frame {
        let start = Instant::now();
        loop {
            let frames = self.frames();
            if let Some(f) = frames.iter().skip(after).find(|f| pred(f)) {
                return f.clone();
            }
            if let Some(status) = self.child.lock().unwrap().try_wait().ok().flatten() {
                dump_fail(
                    "block exited while waiting for a frame",
                    "running",
                    &format!("{status}; stderr={}", self.stderr()),
                    &frames,
                );
            }
            if start.elapsed() > timeout {
                dump_fail(
                    "timed out waiting for a new frame",
                    "matching frame after cursor",
                    &format!("stderr={}", self.stderr()),
                    &frames,
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn track_dir(&mut self, path: PathBuf) {
        self._scratch.push(path);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stub.take();
        for dir in &self._scratch {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

pub enum ClickInstance {
    Latest,
    Value(String),
    Omit,
}

pub fn click_line(button: i32, relative_x: i32, width: i32, instance: Option<&str>) -> String {
    // i3blocks merged-map line. Leading "":"" is what block_send_json writes.
    // full_text is omitted so a glyph cannot confuse parse_click's first-match scan.
    match instance {
        Some(inst) => format!(
            r#"{{"":"","name":"gruve","instance":"{}","button":{button},"relative_x":{relative_x},"width":{width}}}"#,
            inst.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        None => format!(
            r#"{{"":"","name":"gruve","button":{button},"relative_x":{relative_x},"width":{width}}}"#
        ),
    }
}

fn write_launcher(path: &Path) {
    let script = r#"#!/bin/sh
log=$GRUVE_TEST_LAUNCHER_LOG
{
  echo "PID $$"
  echo "ARGC $#"
  for a in "$@"; do
    echo "ARG $a"
  done
  echo "END"
} >> "$log"
if [ -n "$GRUVE_TEST_LAUNCHER_SLEEP" ]; then
  sleep "$GRUVE_TEST_LAUNCHER_SLEEP"
fi
if [ -n "$GRUVE_TEST_LAUNCHER_NOISE" ]; then
  echo "NOISE-STDOUT"
  echo "NOISE-STDERR" >&2
  read -t 0.3 _line || true
  echo "STDIN ${_line-}" >> "$log"
fi
exit 0
"#;
    fs::write(path, script).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

pub fn snap(state: &str) -> String {
    format!(
        r#"{{"state":"{state}","pipeline":"idle","error":null,"connected":false,"engine":"stub"}}"#
    )
}

pub fn snap_error(state: &str, error: &str) -> String {
    format!(
        r#"{{"state":"{state}","pipeline":"idle","error":"{error}","connected":false,"engine":"stub"}}"#
    )
}

pub fn link_rel(config: &Path, link: &str, rel_target: &str) {
    let path = config.join(link);
    let _ = fs::remove_file(&path);
    std::os::unix::fs::symlink(rel_target, &path).unwrap();
}

pub fn executable(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

pub fn dir_names(path: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(rd) = fs::read_dir(path) {
        for ent in rd.flatten() {
            names.push(ent.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

pub fn proc_state(pid: u32) -> Option<char> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let rest = stat[end + 1..].trim_start();
    rest.chars().next()
}

pub fn task_states(pid: u32) -> Vec<char> {
    let mut out = Vec::new();
    let dir = format!("/proc/{pid}/task");
    if let Ok(rd) = fs::read_dir(dir) {
        for ent in rd.flatten() {
            if let Some(state) = fs::read_to_string(ent.path().join("stat"))
                .ok()
                .and_then(|stat| {
                    let end = stat.rfind(')')?;
                    stat[end + 1..].trim_start().chars().next()
                })
            {
                out.push(state);
            }
        }
    }
    out
}

pub fn home_entries(home: &Path) -> Vec<String> {
    fn walk(path: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(path) else {
            return;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            out.push(p.strip_prefix(root).unwrap_or(&p).display().to_string());
            if p.is_dir() {
                walk(&p, root, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(home, home, &mut out);
    out.sort();
    out
}

pub fn sleep_until(deadline: Instant) {
    let now = Instant::now();
    if deadline > now {
        std::thread::sleep(deadline - now);
    }
}

/// relative_x that maps to `cell` under the spec formula, for a known cell count.
/// `cell = min((relative_x * cells) / width, cells-1)`.
pub fn rx_for_cell(cell: i32, cells: i32, width: i32) -> i32 {
    // Choose relative_x = cell * width / cells, which divides evenly when width = 10 * cells.
    cell * width / cells
}
