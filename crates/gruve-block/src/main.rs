//! gruve-block: persistent i3blocks voice indicator.
//! Sends only subscribe/start/stop. Never toggle. Never touches the microphone.

mod home;
mod stt;
mod view;

use std::io::{self, Write};
use std::os::fd::RawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use bnuuy_rustblocks::click::{self, Click};
use bnuuy_rustblocks::i3bar::{self, MinWidth};
use bnuuy_rustblocks::pango;
use bnuuy_rustblocks::platform;
use bnuuy_rustblocks::text::sanitize_ascii;
use bnuuy_rustblocks::widget::Expander;

use crate::home::{Home, LinkRead};
use crate::stt::{
    Command as WireCommand, LineEffect, Phase, Reply, ReqPhase, Snapshot, SUBSCRIBE,
};
use crate::view::{NoticeKind, Side, Tag};

const STDIN_CAP: usize = 4 * 1024;
const SUB_CAP: usize = 64 * 1024;

struct Args {
    runtime: PathBuf,
    config_dir: PathBuf,
    launcher: PathBuf,
    print_paths: bool,
}

struct Delivery {
    preset: String,
    method: String,
}

struct OwnedFd(RawFd);

impl Drop for OwnedFd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe { libc::close(self.0) };
            self.0 = -1;
        }
    }
}

impl OwnedFd {
    fn raw(&self) -> RawFd {
        self.0
    }
}

enum ConnectStart {
    Ready(OwnedFd),
    Pending(OwnedFd),
}

enum SubPhase {
    Offline,
    Connecting,
    Writing,
    Reading { saw: bool },
}

struct Sub {
    fd: Option<OwnedFd>,
    phase: SubPhase,
    write_off: usize,
    read_buf: Vec<u8>,
    deadline: Option<Instant>,
    retry_at: Option<Instant>,
}

struct Req {
    fd: OwnedFd,
    kind: WireCommand,
    phase: ReqPhase,
    write_off: usize,
    read_buf: Vec<u8>,
    deadline: Option<Instant>,
}

struct Sem {
    open: bool,
    intent: stt::Intent,
    layout: Vec<(Tag, usize)>,
}

struct App {
    runtime: PathBuf,
    config_dir: PathBuf,
    launcher: PathBuf,
    scale: f64,
    once: bool,
    home: Home,
    expander: Expander,
    side: Side,
    open: bool,
    phase: Phase,
    snap: Option<Snapshot>,
    local_notice: Option<String>,
    notice_state: Option<String>,
    sub: Sub,
    start: Option<Req>,
    stop: Option<Req>,
    last_button1: Option<Instant>,
    children: Vec<Child>,
    generation: u64,
    sem: Option<Sem>,
    last_line: String,
    stdin_buf: Vec<u8>,
    stdin_discard: bool,
    log_path: Option<PathBuf>,
    delivery: Delivery,
    next_reread: Option<Instant>,
    emitted: bool,
}

fn main() -> io::Result<()> {
    let args = parse_args();
    if args.print_paths {
        println!("{}", args.runtime.display());
        println!("{}", args.config_dir.display());
        return Ok(());
    }
    let mut app = App::new(args);
    if !app.once {
        platform::set_nonblocking(libc::STDIN_FILENO)?;
    }
    let now = Instant::now();
    app.sub_connect(now);
    if !app.once {
        app.render();
    }
    if app.once && app.once_settled() {
        app.render();
        return Ok(());
    }
    loop {
        let timeout = app.next_timeout(Instant::now());
        let mut fds = app.pollfds();
        platform::poll(&mut fds, timeout)?;
        let now = Instant::now();
        if app.drive(&fds, now)? {
            return Ok(());
        }
        app.apply_deadlines(now);
        app.reap();
        if !app.once || app.once_settled() {
            app.render();
        }
        if app.once && app.once_settled() {
            return Ok(());
        }
    }
}

fn parse_args() -> Args {
    let mut runtime = None;
    let mut config_dir = None;
    let mut launcher = None;
    let mut print_paths = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runtime" => runtime = args.next().map(PathBuf::from),
            "--config-dir" => config_dir = args.next().map(PathBuf::from),
            "--launcher" => launcher = args.next().map(PathBuf::from),
            "--print-paths" => print_paths = true,
            _ => {}
        }
    }
    Args {
        runtime: runtime.unwrap_or_else(default_runtime),
        config_dir: config_dir.unwrap_or_else(default_config_dir),
        launcher: launcher.unwrap_or_else(default_launcher),
        print_paths,
    }
}

fn default_runtime() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(xdg).join("bnuuy-stt")
    } else {
        PathBuf::from(format!("/tmp/bnuuy-stt-{}", unsafe { libc::getuid() })).join("bnuuy-stt")
    }
}

fn default_config_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("bnuuy-stt")
    } else {
        home_dir().join(".config").join("bnuuy-stt")
    }
}

fn default_launcher() -> PathBuf {
    home_dir().join(".local/bin/bnuuy-stt")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn timescale() -> f64 {
    std::env::var("GRUVE_BLOCK_TIMESCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|f| f.is_finite() && *f > 0.0)
        .unwrap_or(1.0)
}

fn scaled(ms: u64, scale: f64) -> Duration {
    Duration::from_secs_f64((ms as f64 / 1000.0) * scale)
}

impl App {
    fn new(args: Args) -> Self {
        let scale = timescale();
        let hold = scaled(6_000, scale);
        Self {
            runtime: args.runtime,
            config_dir: args.config_dir.clone(),
            launcher: args.launcher,
            scale,
            once: bnuuy_rustblocks::run_once(),
            home: Home::new(args.config_dir),
            expander: Expander::new(hold),
            side: Side::Preset,
            open: false,
            phase: Phase::Offline,
            snap: None,
            local_notice: None,
            notice_state: None,
            sub: Sub {
                fd: None,
                phase: SubPhase::Offline,
                write_off: 0,
                read_buf: Vec::new(),
                deadline: None,
                retry_at: None,
            },
            start: None,
            stop: None,
            last_button1: None,
            children: Vec::new(),
            generation: 0,
            sem: None,
            last_line: String::new(),
            stdin_buf: Vec::new(),
            stdin_discard: false,
            log_path: std::env::var_os("GRUVE_BLOCK_LOG").map(PathBuf::from),
            delivery: Delivery {
                preset: "raw".to_string(),
                method: "?".to_string(),
            },
            next_reread: None,
            emitted: false,
        }
    }

    fn connect_budget(&self) -> Duration {
        scaled(2_000, self.scale)
    }

    fn reconnect_after(&self) -> Duration {
        scaled(1_500, self.scale)
    }

    fn debounce(&self) -> Duration {
        scaled(500, self.scale)
    }

    fn reread_interval(&self) -> Duration {
        scaled(1_000, self.scale)
    }

    fn once_settled(&self) -> bool {
        match self.sub.phase {
            SubPhase::Offline => true,
            SubPhase::Reading { saw: true } => true,
            _ => false,
        }
    }

    fn next_timeout(&self, now: Instant) -> Option<Duration> {
        let mut deadline: Option<Instant> = None;
        let mut consider = |when: Option<Instant>| {
            if let Some(when) = when {
                deadline = Some(match deadline {
                    Some(prev) => prev.min(when),
                    None => when,
                });
            }
        };
        if self.open {
            match self.expander.remaining(now) {
                Some(left) => consider(Some(now + left)),
                None => consider(Some(now)),
            }
            consider(self.next_reread);
        }
        consider(self.sub.retry_at);
        consider(self.sub.deadline);
        if let Some(req) = &self.start {
            consider(req.deadline);
        }
        if let Some(req) = &self.stop {
            consider(req.deadline);
        }
        if !self.children.is_empty() {
            consider(Some(now + scaled(200, self.scale)));
        }
        deadline.map(|when| when.saturating_duration_since(now))
    }

    fn pollfds(&self) -> Vec<libc::pollfd> {
        let mut fds = Vec::new();
        if !self.once {
            fds.push(libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            });
        }
        if let Some(fd) = self.sub.fd.as_ref() {
            let events = match self.sub.phase {
                SubPhase::Connecting | SubPhase::Writing => libc::POLLOUT,
                SubPhase::Reading { .. } => libc::POLLIN,
                SubPhase::Offline => 0,
            };
            if events != 0 {
                fds.push(libc::pollfd {
                    fd: fd.raw(),
                    events,
                    revents: 0,
                });
            }
        }
        for req in [&self.start, &self.stop].into_iter().flatten() {
            let events = match req.phase {
                ReqPhase::Reading => libc::POLLIN,
                ReqPhase::Connecting | ReqPhase::Writing => libc::POLLOUT,
            };
            fds.push(libc::pollfd {
                fd: req.fd.raw(),
                events,
                revents: 0,
            });
        }
        fds
    }

    fn drive(&mut self, fds: &[libc::pollfd], now: Instant) -> io::Result<bool> {
        let stdin = fds.iter().find(|p| p.fd == libc::STDIN_FILENO);
        if let Some(p) = stdin {
            if p.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                if self.read_stdin(now)? {
                    return Ok(true);
                }
            }
        }
        let sub_fd = self.sub.fd.as_ref().map(OwnedFd::raw);
        if let Some(fd) = sub_fd {
            if let Some(p) = fds.iter().find(|p| p.fd == fd) {
                if p.revents != 0 {
                    self.on_sub(p.revents, now);
                }
            }
        }
        self.on_reqs(fds, now);
        Ok(false)
    }

    fn read_stdin(&mut self, now: Instant) -> io::Result<bool> {
        let mut tmp = [0u8; 4096];
        loop {
            let n = unsafe {
                libc::read(
                    libc::STDIN_FILENO,
                    tmp.as_mut_ptr().cast(),
                    tmp.len(),
                )
            };
            if n < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                if err.kind() == io::ErrorKind::WouldBlock {
                    return Ok(false);
                }
                return Err(err);
            }
            if n == 0 {
                return Ok(true);
            }
            self.ingest_stdin(&tmp[..n as usize], now);
        }
    }

    fn ingest_stdin(&mut self, data: &[u8], now: Instant) {
        for &byte in data {
            if self.stdin_discard {
                if byte == b'\n' {
                    self.stdin_discard = false;
                }
                continue;
            }
            if byte == b'\n' {
                let line = std::mem::take(&mut self.stdin_buf);
                if let Ok(text) = std::str::from_utf8(&line) {
                    let click = click::parse_click(text);
                    self.handle_click(&click, now);
                }
                continue;
            }
            if self.stdin_buf.len() >= STDIN_CAP {
                self.stdin_buf.clear();
                self.stdin_discard = true;
                continue;
            }
            self.stdin_buf.push(byte);
        }
    }

    fn handle_click(&mut self, click: &Click, now: Instant) {
        let current = instance_token(self.generation);
        if click.instance.as_deref() != Some(current.as_str()) {
            self.log_click(click, "stale", None, None, None);
            return;
        }
        if !(1..=5).contains(&click.button) {
            self.log_click(click, "ignore", None, None, None);
            return;
        }
        match click.button {
            1 => self.click_left(click, now),
            2 => {
                self.spawn_configurator();
                if self.open {
                    self.do_collapse();
                }
                self.log_click(click, "spawn", None, None, None);
            }
            3 => {
                if self.open {
                    self.do_collapse();
                    self.log_click(click, "collapse", None, None, None);
                } else {
                    self.do_expand(now);
                    self.log_click(click, "expand", None, None, None);
                }
            }
            4 | 5 => {
                if !self.open {
                    self.log_click(click, "ignore", None, None, None);
                } else {
                    let step = if click.button == 4 { -1 } else { 1 };
                    self.cycle(step);
                    self.expander.touch(now);
                    self.log_click(click, "cycle", None, None, None);
                }
            }
            _ => {}
        }
        self.render();
    }

    fn click_left(&mut self, click: &Click, now: Instant) {
        if self.open {
            let row = self.row();
            let cells = row.cells();
            let cell = click::cell_at(click, cells);
            let hit = row.hit(click);
            match hit {
                Some(Tag::Glyph | Tag::Gap | Tag::Preset) => self.side = Side::Preset,
                Some(Tag::Method | Tag::Notice) => self.side = Side::Method,
                Some(Tag::Arrow) | None => {}
            }
            self.expander.touch(now);
            let verdict = if matches!(hit, Some(Tag::Arrow) | None) {
                "touch"
            } else {
                "side"
            };
            self.log_click(click, verdict, Some(cells), cell, hit);
            return;
        }
        match self.phase.intent() {
            stt::Intent::None => self.log_click(click, "ignore", None, None, None),
            stt::Intent::Start => {
                if self.debounced(now) {
                    self.log_click(click, "debounce", None, None, None);
                } else if !stt::allow_start(self.start.is_some(), self.stop.is_some()) {
                    self.log_click(click, "inflight", None, None, None);
                } else {
                    self.begin_request(WireCommand::Start, now);
                    self.log_click(click, "start", None, None, None);
                }
            }
            stt::Intent::Stop => {
                if self.debounced(now) {
                    self.log_click(click, "debounce", None, None, None);
                } else if !stt::allow_stop(self.start.as_ref().map(|r| r.phase), self.stop.is_some())
                {
                    self.log_click(click, "inflight", None, None, None);
                } else {
                    self.begin_request(WireCommand::Stop, now);
                    self.log_click(click, "stop", None, None, None);
                }
            }
        }
    }

    fn debounced(&self, now: Instant) -> bool {
        self.last_button1
            .is_some_and(|then| now.saturating_duration_since(then) < self.debounce())
    }

    fn do_expand(&mut self, now: Instant) {
        self.side = Side::Preset;
        self.expander.expand(now);
        self.open = true;
        self.reread_links();
        self.next_reread = Some(now + self.reread_interval());
    }

    fn do_collapse(&mut self) {
        self.expander.collapse();
        self.open = false;
        self.side = Side::Preset;
        self.local_notice = None;
        self.notice_state = None;
        self.next_reread = None;
    }

    fn cycle(&mut self, step: i32) {
        let result = if self.side == Side::Preset {
            self.home.cycle_preset(step)
        } else {
            self.home.cycle_input(step)
        };
        if result.is_err() {
            self.set_notice("symlink error");
        }
        self.reread_links();
    }

    fn reread_links(&mut self) {
        match self.home.read_preset() {
            LinkRead::Absent => self.delivery.preset = "raw".to_string(),
            LinkRead::Name(name) => self.delivery.preset = view::display_os(&name),
            LinkRead::Failed => {
                self.delivery.preset = "?".to_string();
                self.set_notice("symlink error");
            }
        }
        match self.home.read_input() {
            LinkRead::Absent => self.delivery.method = "?".to_string(),
            LinkRead::Name(name) => {
                self.delivery.method = view::abbreviate_input(&name.to_string_lossy());
            }
            LinkRead::Failed => {
                self.delivery.method = "?".to_string();
                self.set_notice("symlink error");
            }
        }
    }

    fn set_notice(&mut self, text: &str) {
        self.local_notice = Some(sanitize_ascii(text));
        self.notice_state = self.snap.as_ref().map(|snap| snap.state.clone());
    }

    fn apply_snap(&mut self, snap: Snapshot) {
        let state = snap.state.clone();
        let clear = match &self.notice_state {
            Some(prev) => *prev != state,
            None => self.local_notice.is_some(),
        };
        if clear {
            self.local_notice = None;
            self.notice_state = None;
        }
        self.phase = stt::phase_of(&snap);
        self.snap = Some(snap);
    }

    fn fail_closed_unknown(&mut self) {
        self.phase = Phase::Unknown;
        self.snap = None;
    }

    fn spawn_configurator(&mut self) {
        let config = self.config_dir.join("config.toml");
        let mut cmd = Command::new(&self.launcher);
        cmd.arg("--config")
            .arg(&config)
            .arg("configure")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        match cmd.spawn() {
            Ok(child) => self.children.push(child),
            Err(_) => self.set_notice("spawn failed"),
        }
    }

    fn reap(&mut self) {
        self.children.retain_mut(|child| match child.try_wait() {
            Ok(None) => true,
            _ => false,
        });
    }

    fn sub_connect(&mut self, now: Instant) {
        let path = self.runtime.join("control.sock");
        match unix_connect(&path) {
            Ok(ConnectStart::Ready(fd)) => {
                self.sub.fd = Some(fd);
                self.sub.phase = SubPhase::Writing;
                self.sub.write_off = 0;
                self.sub.read_buf.clear();
                self.sub.deadline = Some(now + self.connect_budget());
                self.sub.retry_at = None;
                self.pump_sub_write(now);
            }
            Ok(ConnectStart::Pending(fd)) => {
                self.sub.fd = Some(fd);
                self.sub.phase = SubPhase::Connecting;
                self.sub.write_off = 0;
                self.sub.read_buf.clear();
                self.sub.deadline = Some(now + self.connect_budget());
                self.sub.retry_at = None;
            }
            Err(_) => self.sub_down(now, true),
        }
    }

    fn sub_down(&mut self, now: Instant, notice: bool) {
        self.sub.fd = None;
        self.sub.phase = SubPhase::Offline;
        self.sub.write_off = 0;
        self.sub.read_buf.clear();
        self.sub.deadline = None;
        self.sub.retry_at = if self.once {
            None
        } else {
            Some(now + self.reconnect_after())
        };
        self.phase = Phase::Offline;
        self.snap = None;
        if notice {
            self.set_notice("controller unreachable");
        }
    }

    fn on_sub(&mut self, revents: i16, now: Instant) {
        let err = revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0;
        let out = revents & libc::POLLOUT != 0;
        let inn = revents & libc::POLLIN != 0;
        match self.sub.phase {
            SubPhase::Connecting if out || err => {
                let fd = self.sub.fd.as_ref().map(OwnedFd::raw);
                let Some(fd) = fd else { return };
                if platform::so_error(fd).is_err() {
                    self.sub_down(now, true);
                    return;
                }
                self.sub.phase = SubPhase::Writing;
                self.sub.write_off = 0;
                self.sub.deadline = Some(now + self.connect_budget());
                self.pump_sub_write(now);
            }
            SubPhase::Writing if out || err => self.pump_sub_write(now),
            SubPhase::Reading { .. } if inn || err => self.pump_sub_read(now),
            _ => {}
        }
    }

    fn pump_sub_write(&mut self, now: Instant) {
        let Some(fd) = self.sub.fd.as_ref().map(OwnedFd::raw) else {
            return;
        };
        match write_some(fd, SUBSCRIBE, &mut self.sub.write_off) {
            Ok(true) => {
                self.sub.phase = SubPhase::Reading { saw: false };
                self.sub.deadline = Some(now + self.connect_budget());
            }
            Ok(false) => {}
            Err(_) => self.sub_down(now, true),
        }
    }

    fn pump_sub_read(&mut self, now: Instant) {
        let Some(fd) = self.sub.fd.as_ref().map(OwnedFd::raw) else {
            return;
        };
        match read_some(fd, &mut self.sub.read_buf, SUB_CAP) {
            Ok(ReadEnd::Oversize) => self.sub_down(now, true),
            Ok(ReadEnd::Error) => self.sub_down(now, true),
            Ok(ReadEnd::Eof) => {
                self.take_sub_lines();
                self.sub_down(now, true);
            }
            Ok(ReadEnd::Block) => {
                self.take_sub_lines();
            }
            Err(_) => self.sub_down(now, true),
        }
    }

    fn take_sub_lines(&mut self) {
        if self.sub.read_buf.len() >= SUB_CAP && !self.sub.read_buf.contains(&b'\n') {
            return;
        }
        for line in take_lines(&mut self.sub.read_buf) {
            let Ok(text) = std::str::from_utf8(&line) else {
                self.fail_closed_unknown();
                self.mark_sub_saw();
                continue;
            };
            self.mark_sub_saw();
            match stt::apply_line(text) {
                LineEffect::Known(snap) => self.apply_snap(snap),
                LineEffect::Unknown => {
                    self.fail_closed_unknown();
                    self.log_text(&format!("bad-line {text}\n"));
                }
            }
            self.render();
        }
    }

    fn mark_sub_saw(&mut self) {
        if let SubPhase::Reading { saw } = &mut self.sub.phase {
            *saw = true;
        }
        self.sub.deadline = None;
    }

    fn begin_request(&mut self, kind: WireCommand, now: Instant) {
        let path = self.runtime.join("control.sock");
        let (fd, phase) = match unix_connect(&path) {
            Ok(ConnectStart::Ready(fd)) => (fd, ReqPhase::Writing),
            Ok(ConnectStart::Pending(fd)) => (fd, ReqPhase::Connecting),
            Err(_) => {
                self.set_notice("controller unreachable");
                return;
            }
        };
        let mut req = Req {
            fd,
            kind,
            phase,
            write_off: 0,
            read_buf: Vec::new(),
            deadline: Some(now + self.connect_budget()),
        };
        if phase == ReqPhase::Writing {
            match write_some(req.fd.raw(), kind.wire(), &mut req.write_off) {
                Ok(true) => {
                    req.phase = ReqPhase::Reading;
                    req.deadline = None;
                }
                Ok(false) => {}
                Err(_) => {
                    self.set_notice("controller unreachable");
                    return;
                }
            }
        }
        self.last_button1 = Some(now);
        match kind {
            WireCommand::Start => self.start = Some(req),
            WireCommand::Stop => self.stop = Some(req),
        }
    }

    fn on_reqs(&mut self, fds: &[libc::pollfd], now: Instant) {
        let start_fd = self.start.as_ref().map(|r| r.fd.raw());
        let stop_fd = self.stop.as_ref().map(|r| r.fd.raw());
        if let Some(fd) = start_fd {
            if let Some(p) = fds.iter().find(|p| p.fd == fd) {
                if p.revents != 0 {
                    self.on_req(true, p.revents, now);
                }
            }
        }
        if let Some(fd) = stop_fd {
            if let Some(p) = fds.iter().find(|p| p.fd == fd) {
                if p.revents != 0 {
                    self.on_req(false, p.revents, now);
                }
            }
        }
    }

    fn on_req(&mut self, is_start: bool, revents: i16, now: Instant) {
        let Some(req) = (if is_start { &self.start } else { &self.stop }) else {
            return;
        };
        let phase = req.phase;
        let fd = req.fd.raw();
        let err = revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0;
        let out = revents & libc::POLLOUT != 0;
        let inn = revents & libc::POLLIN != 0;
        match phase {
            ReqPhase::Connecting if out || err => {
                if platform::so_error(fd).is_err() {
                    self.clear_req(is_start, "controller unreachable");
                    return;
                }
                let budget = self.connect_budget();
                if let Some(req) = self.req_mut(is_start) {
                    req.phase = ReqPhase::Writing;
                    req.write_off = 0;
                    req.deadline = Some(now + budget);
                }
                self.pump_req_write(is_start);
            }
            ReqPhase::Writing if out || err => self.pump_req_write(is_start),
            ReqPhase::Reading if inn || err => self.pump_req_read(is_start),
            _ => {}
        }
    }

    fn req_mut(&mut self, is_start: bool) -> Option<&mut Req> {
        if is_start {
            self.start.as_mut()
        } else {
            self.stop.as_mut()
        }
    }

    fn pump_req_write(&mut self, is_start: bool) {
        let (kind, fd, mut off) = {
            let Some(req) = self.req_mut(is_start) else {
                return;
            };
            (req.kind, req.fd.raw(), req.write_off)
        };
        match write_some(fd, kind.wire(), &mut off) {
            Ok(done) => {
                if let Some(req) = self.req_mut(is_start) {
                    req.write_off = off;
                    if done {
                        req.phase = ReqPhase::Reading;
                        req.deadline = None;
                    }
                }
            }
            Err(_) => self.clear_req(is_start, "controller unreachable"),
        }
    }

    fn pump_req_read(&mut self, is_start: bool) {
        let end = {
            let Some(req) = self.req_mut(is_start) else {
                return;
            };
            let fd = req.fd.raw();
            read_some(fd, &mut req.read_buf, SUB_CAP)
        };
        match end {
            Ok(ReadEnd::Block) => {
                self.take_req_lines(is_start);
            }
            Ok(ReadEnd::Eof) => {
                let had = self.take_req_lines(is_start);
                if !had {
                    self.clear_req(is_start, "no reply");
                }
            }
            Ok(ReadEnd::Oversize) | Ok(ReadEnd::Error) | Err(_) => {
                self.clear_req(is_start, "malformed reply");
            }
        }
    }

    fn take_req_lines(&mut self, is_start: bool) -> bool {
        let parsed = {
            let Some(req) = self.req_mut(is_start) else {
                return false;
            };
            let lines = take_lines(&mut req.read_buf);
            if lines.is_empty() {
                return false;
            }
            let kind = req.kind;
            let text = String::from_utf8_lossy(&lines[0]).into_owned();
            (kind, text)
        };
        self.finish_reply(is_start, parsed.0, &parsed.1);
        true
    }

    fn finish_reply(&mut self, is_start: bool, _kind: WireCommand, line: &str) {
        if is_start {
            self.start = None;
        } else {
            self.stop = None;
        }
        match stt::parse_reply(line) {
            Reply::Applied(LineEffect::Known(snap)) => self.apply_snap(snap),
            Reply::Applied(LineEffect::Unknown) => self.fail_closed_unknown(),
            Reply::Rejected(err) => self.set_notice(&err),
            Reply::Malformed => self.set_notice("malformed reply"),
        }
        self.render();
    }

    fn clear_req(&mut self, is_start: bool, notice: &str) {
        if is_start {
            self.start = None;
        } else {
            self.stop = None;
        }
        self.set_notice(notice);
        self.render();
    }

    fn apply_deadlines(&mut self, now: Instant) {
        if self.open && self.expander.remaining(now).is_none() {
            self.do_collapse();
        }
        if self.open && self.next_reread.is_some_and(|when| now >= when) {
            self.reread_links();
            self.next_reread = Some(now + self.reread_interval());
        }
        if self.sub.deadline.is_some_and(|when| now >= when) {
            self.sub_down(now, true);
        }
        if matches!(self.sub.phase, SubPhase::Offline)
            && self.sub.retry_at.is_some_and(|when| now >= when)
            && !self.once
        {
            self.sub.retry_at = None;
            self.sub_connect(now);
        }
        if self
            .start
            .as_ref()
            .and_then(|req| req.deadline)
            .is_some_and(|when| now >= when)
        {
            self.clear_req(true, "controller unreachable");
        }
        if self
            .stop
            .as_ref()
            .and_then(|req| req.deadline)
            .is_some_and(|when| now >= when)
        {
            self.clear_req(false, "controller unreachable");
        }
    }

    fn notice(&self) -> Option<NoticeKind> {
        if let Some(err) = self.snap.as_ref().and_then(|snap| snap.error.clone()) {
            return Some(NoticeKind::Error(sanitize_ascii(&err)));
        }
        if let Some(local) = &self.local_notice {
            return Some(NoticeKind::Local(local.clone()));
        }
        if let Some(snap) = &self.snap {
            if snap.pipeline != "idle" {
                return Some(NoticeKind::Pipeline(sanitize_ascii(&snap.pipeline)));
            }
        }
        None
    }

    fn row(&self) -> bnuuy_rustblocks::widget::Row<Tag> {
        let notice = self.notice();
        view::build_row(
            self.phase,
            &self.delivery.preset,
            &self.delivery.method,
            self.side,
            notice.as_ref(),
        )
    }

    fn render(&mut self) {
        let look = view::look(self.phase);
        let (full_text, layout) = if self.open {
            let row = self.row();
            let layout = row.layout();
            (row.markup(), layout)
        } else {
            (
                pango::format(look.glyph, 0, Some(&view::span(look.color))),
                Vec::new(),
            )
        };
        let sem = Sem {
            open: self.open,
            intent: look.intent,
            layout,
        };
        let changed = match &self.sem {
            None => true,
            Some(prev) => !same_sem(prev, &sem),
        };
        if self.generation == 0 || changed {
            self.generation = self.generation.saturating_add(1);
            self.sem = Some(sem);
        }
        let mut block = i3bar::Block::pango(full_text);
        block.instance = Some(instance_token(self.generation));
        if !self.open {
            block.min_width = Some(MinWidth::Text("◯".to_string()));
        }
        let line = block.to_json_line();
        if line != self.last_line {
            if i3bar::emit(&block).is_err() {
                std::process::exit(0);
            }
            self.last_line = line;
            self.emitted = true;
        }
    }

    fn log_click(
        &self,
        click: &Click,
        verdict: &str,
        cells: Option<usize>,
        cell: Option<usize>,
        tag: Option<Tag>,
    ) {
        let cells = cells.map(|n| n.to_string()).unwrap_or_else(|| "0".into());
        let cell = cell.map(|n| n.to_string()).unwrap_or_else(|| "-".into());
        let tag = tag.map(view::tag_name).unwrap_or("-");
        self.log_text(&format!(
            "{} {} {} {} {} {} {} {}\n",
            self.generation, click.button, click.relative_x, click.width, cells, cell, tag, verdict
        ));
    }

    fn log_text(&self, line: &str) {
        let Some(path) = &self.log_path else {
            return;
        };
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        else {
            return;
        };
        let _ = file.write_all(line.as_bytes());
    }
}

/// Frame generation as sent in `instance`. i3blocks stores values unquoted and
/// writes them back raw when they are valid JSON (block_send_key), so a bare
/// number would return as a JSON number and never match; the prefix keeps the
/// round trip a string.
fn instance_token(generation: u64) -> String {
    format!("g{generation}")
}

fn same_sem(prev: &Sem, next: &Sem) -> bool {
    if prev.open != next.open {
        return false;
    }
    if next.open {
        prev.layout == next.layout
    } else {
        prev.intent == next.intent
    }
}

enum ReadEnd {
    Block,
    Eof,
    Oversize,
    Error,
}

fn take_lines(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    while let Some(idx) = buf.iter().position(|byte| *byte == b'\n') {
        let mut line: Vec<u8> = buf.drain(..=idx).collect();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        lines.push(line);
    }
    lines
}

fn read_some(fd: RawFd, buf: &mut Vec<u8>, cap: usize) -> io::Result<ReadEnd> {
    let mut tmp = [0u8; 8192];
    loop {
        if buf.len() >= cap && !buf.contains(&b'\n') {
            return Ok(ReadEnd::Oversize);
        }
        let space = cap.saturating_sub(buf.len()).max(1);
        let n = unsafe { libc::read(fd, tmp.as_mut_ptr().cast(), tmp.len().min(space)) };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if err.kind() == io::ErrorKind::WouldBlock {
                if buf.len() >= cap && !buf.contains(&b'\n') {
                    return Ok(ReadEnd::Oversize);
                }
                return Ok(ReadEnd::Block);
            }
            return Ok(ReadEnd::Error);
        }
        if n == 0 {
            if buf.len() >= cap && !buf.contains(&b'\n') {
                return Ok(ReadEnd::Oversize);
            }
            return Ok(ReadEnd::Eof);
        }
        let room = cap.saturating_sub(buf.len());
        let take = (n as usize).min(room);
        if take == 0 {
            return Ok(ReadEnd::Oversize);
        }
        buf.extend_from_slice(&tmp[..take]);
        if (n as usize) > take {
            return Ok(ReadEnd::Oversize);
        }
        if buf.len() >= cap && !buf.contains(&b'\n') {
            return Ok(ReadEnd::Oversize);
        }
    }
}

fn write_some(fd: RawFd, data: &[u8], off: &mut usize) -> io::Result<bool> {
    while *off < data.len() {
        let n = unsafe {
            libc::write(
                fd,
                data[*off..].as_ptr().cast(),
                data.len() - *off,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if err.kind() == io::ErrorKind::WouldBlock {
                return Ok(false);
            }
            return Err(err);
        }
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "short write"));
        }
        *off += n as usize;
    }
    Ok(true)
}

fn unix_connect(path: &Path) -> io::Result<ConnectStart> {
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let owned = OwnedFd(fd);
    platform::set_nonblocking(owned.raw())?;
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str());
    if bytes.contains(&0) || bytes.len() >= 108 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "socket path",
        ));
    }
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (idx, byte) in bytes.iter().enumerate() {
        addr.sun_path[idx] = *byte as libc::c_char;
    }
    let len = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
    let rc = unsafe {
        libc::connect(
            owned.raw(),
            (&addr as *const libc::sockaddr_un).cast(),
            len,
        )
    };
    if rc == 0 {
        return Ok(ConnectStart::Ready(owned));
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::EINPROGRESS)
        || err.raw_os_error() == Some(libc::EAGAIN)
        || err.kind() == io::ErrorKind::WouldBlock
    {
        return Ok(ConnectStart::Pending(owned));
    }
    Err(err)
}
