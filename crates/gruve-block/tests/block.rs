//! Acceptance suite A1–A12. Drives `gruve-block` as a black box.
//! Cell counts are literals from the font table, never `text::cells`.

mod common;

use common::*;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(400);
const FRAME_WAIT: Duration = Duration::from_secs(2);

fn push_and_wait(world: &World, line: &str, pred_plain: &str) -> common::Frame {
    world.stub().wait_subscribes(1, FRAME_WAIT);
    world.stub().push_line(line);
    world.wait_plain(pred_plain, FRAME_WAIT)
}

fn assert_collapsed_glyph(world: &World, frame: &common::Frame, glyph: &str, hex: &str) {
    world.assert_frame_contract(frame, false);
    if frame.plain != glyph {
        dump_fail(
            "collapsed glyph",
            glyph,
            &frame.plain,
            &world.frames(),
        );
    }
    if !frame.full_text.contains(hex) {
        dump_fail(
            "collapsed color hex in full_text",
            hex,
            &frame.full_text,
            &world.frames(),
        );
    }
    if frame.instance.as_deref().unwrap_or("").is_empty() {
        dump_fail("instance", "non-empty decimal gen", "<missing>", &[frame.clone()]);
    }
}

fn expand(world: &mut World) -> common::Frame {
    let before = world.frames().len();
    world.click_latest(3, 0, 100);
    world.wait_frame(FRAME_WAIT, move |f| {
        f.min_width.is_none() && world_expanded(f)
    });
    // wait_frame's closure can't capture before reliably if we move. Re-fetch.
    let frames = world.frames();
    frames
        .iter()
        .skip(before)
        .rev()
        .find(|f| f.min_width.is_none() && f.plain.chars().count() > 1)
        .cloned()
        .unwrap_or_else(|| world.latest())
}

fn world_expanded(f: &common::Frame) -> bool {
    f.plain.contains('→') || f.plain.chars().count() > 1
}

fn settle_no_request(world: &World, before: usize, what: &str) {
    std::thread::sleep(SETTLE);
    let got = world.stub().requests();
    if got.len() != before {
        dump_fail(
            what,
            &format!("{before} request(s)"),
            &format!("{got:?}"),
            &world.frames(),
        );
    }
}

fn request_commands(world: &World) -> Vec<String> {
    world
        .stub()
        .requests()
        .iter()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| v.get("command").and_then(|c| c.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| l.clone())
        })
        .collect()
}

// --- A1 -------------------------------------------------------------------

#[test]
fn a1_glyph_table() {
    // §1 rows, plus idle+error (warned), unknown, missing state, no daemon.
    struct Case {
        name: &'static str,
        line: Option<&'static str>,
        glyph: &'static str,
        hex: &'static str,
    }
    let cases = [
        Case { name: "idle", line: Some(r#"{"state":"idle","error":null}"#), glyph: GLYPH_IDLE, hex: BLUE },
        Case { name: "idle+error", line: Some(r#"{"state":"idle","error":"stage warning"}"#), glyph: GLYPH_IDLE, hex: YELLOW },
        Case { name: "starting", line: Some(r#"{"state":"starting"}"#), glyph: GLYPH_STARTING, hex: BLUE },
        Case { name: "recording", line: Some(r#"{"state":"recording"}"#), glyph: GLYPH_ACTIVE, hex: BLUE },
        Case { name: "connected", line: Some(r#"{"state":"connected"}"#), glyph: GLYPH_ACTIVE, hex: BLUE },
        Case { name: "transcribing", line: Some(r#"{"state":"transcribing"}"#), glyph: GLYPH_ACTIVE, hex: FG_DIM },
        Case { name: "delivering", line: Some(r#"{"state":"delivering"}"#), glyph: GLYPH_ACTIVE, hex: FG_DIM },
        Case { name: "error", line: Some(r#"{"state":"error"}"#), glyph: GLYPH_IDLE, hex: RED },
        Case { name: "unknown-state", line: Some(r#"{"state":"nope"}"#), glyph: GLYPH_UNKNOWN, hex: ORANGE },
        Case { name: "missing-state", line: Some(r#"{"pipeline":"idle"}"#), glyph: GLYPH_UNKNOWN, hex: ORANGE },
        Case { name: "empty-state", line: Some(r#"{"state":""}"#), glyph: GLYPH_UNKNOWN, hex: ORANGE },
        Case { name: "malformed", line: Some("not-json"), glyph: GLYPH_UNKNOWN, hex: ORANGE },
    ];

    for case in cases {
        let mut world = World::start();
        world.stub().wait_subscribes(1, FRAME_WAIT);
        world.stub().push_line(case.line.unwrap());
        let frame = world.wait_plain(case.glyph, FRAME_WAIT);
        assert_collapsed_glyph(&world, &frame, case.glyph, case.hex);
        if !frame.full_text.contains(case.hex) {
            dump_fail(case.name, case.hex, &frame.full_text, &world.frames());
        }
        // Expanded frames carry neither min_width nor border*.
        let expanded = expand(&mut world);
        world.assert_frame_contract(&expanded, true);
        if !expanded.plain.contains(case.glyph) {
            dump_fail(
                &format!("{} expanded glyph", case.name),
                case.glyph,
                &expanded.plain,
                &world.frames(),
            );
        }
        if !expanded.full_text.contains(case.hex) {
            dump_fail(
                &format!("{} expanded color", case.name),
                case.hex,
                &expanded.full_text,
                &world.frames(),
            );
        }
        world.assert_no_bad_lines();
        world.assert_wire();
    }

    let mut offline = offline_world();
    let frame = offline.wait_plain(GLYPH_OFFLINE, FRAME_WAIT);
    assert_collapsed_glyph(&offline, &frame, GLYPH_OFFLINE, FG_DIM);
    let expanded = expand(&mut offline);
    offline.assert_frame_contract(&expanded, true);
    if expanded.min_width.is_some() {
        dump_fail("offline expanded min_width", "absent", "present", &offline.frames());
    }
    offline.assert_no_bad_lines();
}

/// Socket directory exists, socket file does not. `--runtime` is that temp dir,
/// never `$XDG_RUNTIME_DIR/bnuuy-stt`.
fn offline_world() -> World {
    let runtime = std::env::temp_dir().join(format!(
        "gruve-block-offline-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&runtime).unwrap();
    let mut world = World::start_with(WorldOpts {
        with_stub: false,
        args: vec!["--runtime".into(), runtime.display().to_string()],
        ..WorldOpts::default()
    });
    world.track_dir(runtime);
    world
}

// --- A2 -------------------------------------------------------------------

#[test]
fn a2_wire_vocabulary() {
    let mut world = World::start();
    world.stub().set_reply(&snap("recording"));
    let idle = push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    let _ = idle;
    world.click_latest(1, 0, 20);
    let reqs = world.stub().wait_requests(1, FRAME_WAIT);
    assert!(
        reqs[0].contains("start") && !reqs[0].contains("toggle"),
        "expected start, got {}",
        reqs[0]
    );
    // Subscription publishes recording; reply also says recording.
    world.stub().push_line(&snap("recording"));
    world.wait_plain(GLYPH_ACTIVE, FRAME_WAIT);
    std::thread::sleep(DEBOUNCE + Duration::from_millis(30));
    world.click_latest(1, 0, 20);
    let reqs = world.stub().wait_requests(2, FRAME_WAIT);
    assert!(reqs[1].contains("stop"), "expected stop, got {}", reqs[1]);
    std::thread::sleep(Duration::from_millis(50));
    let names = world.stub().command_names();
    for name in &names {
        assert!(
            name == "subscribe" || name == "start" || name == "stop",
            "illegal command {name}; lines={:?}",
            world.stub().all_lines()
        );
    }
    assert!(names.contains(&"subscribe".to_string()));
    assert!(names.contains(&"start".to_string()));
    assert!(names.contains(&"stop".to_string()));
    assert!(!names.iter().any(|n| n == "toggle"));
    world.assert_wire();
}

// --- A3 -------------------------------------------------------------------

#[test]
fn a3_intent_gating() {
    struct Case {
        name: &'static str,
        line: Option<&'static str>,
        glyph: &'static str,
        expect: Option<&'static str>,
    }
    let cases = [
        Case { name: "idle", line: Some(r#"{"state":"idle","error":null}"#), glyph: GLYPH_IDLE, expect: Some("start") },
        Case { name: "error", line: Some(r#"{"state":"error","error":"boom"}"#), glyph: GLYPH_IDLE, expect: Some("start") },
        Case { name: "idle+warning", line: Some(r#"{"state":"idle","error":"warn"}"#), glyph: GLYPH_IDLE, expect: Some("start") },
        Case { name: "recording", line: Some(r#"{"state":"recording"}"#), glyph: GLYPH_ACTIVE, expect: Some("stop") },
        Case { name: "connected", line: Some(r#"{"state":"connected"}"#), glyph: GLYPH_ACTIVE, expect: Some("stop") },
        Case { name: "starting", line: Some(r#"{"state":"starting"}"#), glyph: GLYPH_STARTING, expect: None },
        Case { name: "transcribing", line: Some(r#"{"state":"transcribing"}"#), glyph: GLYPH_ACTIVE, expect: None },
        Case { name: "delivering", line: Some(r#"{"state":"delivering"}"#), glyph: GLYPH_ACTIVE, expect: None },
        Case { name: "unknown", line: Some(r#"{"state":"nope"}"#), glyph: GLYPH_UNKNOWN, expect: None },
        Case { name: "malformed", line: Some("not-json"), glyph: GLYPH_UNKNOWN, expect: None },
    ];
    for case in cases {
        let mut world = World::start();
        world.stub().set_withhold();
        world.stub().wait_subscribes(1, FRAME_WAIT);
        world.stub().push_line(case.line.unwrap());
        world.wait_plain(case.glyph, FRAME_WAIT);
        // Yellow vs blue idle both render ◯; color distinguishes warned, but the
        // click only cares about the glyph state. Wait until the color is right
        // so we don't click the initial offline frame (also not ◯).
        world.click_latest(1, 0, 40);
        match case.expect {
            Some(cmd) => {
                let reqs = world.stub().wait_requests(1, FRAME_WAIT);
                let got = &reqs[0];
                if !got.contains(&format!("\"command\":\"{cmd}\"")) && !got.contains(&format!("\"command\": \"{cmd}\"")) {
                    dump_fail(case.name, cmd, got, &world.frames());
                }
                std::thread::sleep(DEBOUNCE + Duration::from_millis(40));
                if world.stub().requests().len() != 1 {
                    dump_fail(case.name, "exactly one request", &format!("{:?}", world.stub().requests()), &world.frames());
                }
            }
            None => settle_no_request(&world, 0, case.name),
        }
        world.assert_wire();
    }

    let mut offline = offline_world();
    offline.wait_plain(GLYPH_OFFLINE, FRAME_WAIT);
    offline.click_latest(1, 0, 40);
    // No stub: nothing to record. Give the block time to misbehave by connecting
    // somewhere. Our --runtime has no socket, so a start would have to open one.
    std::thread::sleep(SETTLE);
    // If it created a request socket in the offline runtime, a listener isn't there;
    // the observable is that it stays offline and does not spawn the launcher.
    assert!(
        offline.launcher_records().is_empty(),
        "offline button 1 must not spawn anything"
    );
    let latest = offline.latest();
    if latest.plain != GLYPH_OFFLINE {
        dump_fail("offline button 1", GLYPH_OFFLINE, &latest.plain, &offline.frames());
    }
}

// --- A4 -------------------------------------------------------------------

#[test]
fn a4a_debounce_double_click() {
    // Two button-1 lines 4 ms apart (under the scaled 25 ms debounce; spec's
    // "scaled 100 ms" gap) → one request.
    let mut world = World::start();
    world.stub().set_withhold();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(1, 0, 40);
    std::thread::sleep(Duration::from_millis(4));
    world.click_latest(1, 0, 40);
    let reqs = world.stub().wait_requests(1, FRAME_WAIT);
    assert!(reqs[0].contains("start"), "got {}", reqs[0]);
    std::thread::sleep(DEBOUNCE + Duration::from_millis(40));
    assert_eq!(world.stub().requests().len(), 1, "debounce leaked a second request: {:?}", world.stub().requests());
    world.assert_wire();
}

#[test]
fn a4b_withhold_start_ignores_button1_expand_ok() {
    let mut world = World::start();
    world.stub().set_withhold();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(1, 0, 40);
    world.stub().wait_requests(1, FRAME_WAIT);
    world.stub().push_line(&snap("starting"));
    world.wait_plain(GLYPH_STARTING, FRAME_WAIT);
    std::thread::sleep(DEBOUNCE + Duration::from_millis(30));
    world.click_latest(1, 0, 40);
    settle_no_request(&world, 1, "button 1 while start in flight");
    world.click_latest(3, 0, 100);
    let expanded = world.wait_frame(FRAME_WAIT, |f| f.min_width.is_none() && f.plain.contains('→'));
    world.assert_frame_contract(&expanded, true);
    settle_no_request(&world, 1, "button 3 while start in flight");
    world.assert_wire();
}

#[test]
fn a4c_stop_while_start_reply_withheld() {
    let mut world = World::start();
    world.stub().set_withhold();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(1, 0, 40);
    world.stub().wait_requests(1, FRAME_WAIT);
    world.stub().push_line(&snap("recording"));
    world.wait_plain(GLYPH_ACTIVE, FRAME_WAIT);
    std::thread::sleep(DEBOUNCE + Duration::from_millis(30));
    world.click_latest(1, 0, 40);
    let reqs = world.stub().wait_requests(2, FRAME_WAIT);
    assert!(reqs[1].contains("stop"), "expected stop, got {}", reqs[1]);
    std::thread::sleep(DEBOUNCE + Duration::from_millis(40));
    world.click_latest(1, 0, 40);
    settle_no_request(&world, 2, "second button 1 while stop in flight");
    world.assert_wire();
}

#[test]
fn a4d_no_reply_deadline() {
    // 20 × the old 10 s reply figure, scaled by 0.05 = 10 s wall.
    // Still in flight, no ORANGE notice.
    let mut world = World::start();
    world.stub().set_withhold();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(1, 0, 40);
    world.stub().wait_requests(1, FRAME_WAIT);
    world.stub().push_line(&snap("starting"));
    world.wait_plain(GLYPH_STARTING, FRAME_WAIT);
    world.click_latest(3, 0, 100);
    let expanded = world.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    if expanded.full_text.contains(ORANGE) || expanded.plain.contains("no reply") {
        dump_fail("notice before the wait", "no notice", &expanded.full_text, &world.frames());
    }
    let start = Instant::now();
    let budget = Duration::from_secs(10);
    while start.elapsed() < budget {
        if world.stub().open_requests() == 0 {
            dump_fail(
                "request cleared before 10s",
                "still in flight",
                "socket closed",
                &world.frames(),
            );
        }
        for frame in world.frames() {
            if frame.full_text.contains(ORANGE) || frame.plain.contains("no reply") || frame.plain.contains("controller unreachable") {
                dump_fail("ORANGE notice while reply withheld", "no notice", &frame.plain, &world.frames());
            }
        }
        if request_commands(&world).len() != 1 {
            dump_fail("extra request during withhold", "[start]", &format!("{:?}", request_commands(&world)), &world.frames());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(world.stub().open_requests() >= 1, "request not still open after 10s");
    world.assert_wire();
}

#[test]
fn a4e_close_without_reply_then_retry() {
    let mut world = World::start();
    world.stub().set_close_without_reply();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(1, 0, 40);
    world.stub().wait_requests(1, FRAME_WAIT);
    world.click_latest(3, 0, 100);
    let expanded = world.wait_frame(FRAME_WAIT, |f| {
        f.min_width.is_none() && f.full_text.contains(ORANGE) && f.plain.contains("no reply")
    });
    if !expanded.plain.contains(GLYPH_IDLE) {
        dump_fail("glyph after no reply", GLYPH_IDLE, &expanded.plain, &world.frames());
    }
    // Collapse, then an eligible click sends again.
    let mark = world.frame_len();
    world.click_latest(3, 0, 100);
    world.wait_after(mark, FRAME_WAIT, |f| f.min_width.is_some() && f.plain == GLYPH_IDLE);
    std::thread::sleep(DEBOUNCE + Duration::from_millis(30));
    world.click_latest(1, 0, 40);
    let reqs = world.stub().wait_requests(2, FRAME_WAIT);
    assert!(reqs[1].contains("start"), "retry should start, got {}", reqs[1]);
    world.assert_wire();
}

#[test]
fn a4f_stop_reply_result_becomes_idle() {
    let mut world = World::start();
    // Subscription stays on recording; the stop reply carries idle.
    world.stub().set_reply(r#"{"ok":true,"result":{"state":"idle","pipeline":"idle","error":null}}"#);
    push_and_wait(&world, &snap("recording"), GLYPH_ACTIVE);
    world.click_latest(1, 0, 40);
    world.stub().wait_requests(1, FRAME_WAIT);
    let idle = world.wait_plain(GLYPH_IDLE, FRAME_WAIT);
    if !idle.full_text.contains(BLUE) {
        dump_fail("reply result idle color", BLUE, &idle.full_text, &world.frames());
    }
    std::thread::sleep(DEBOUNCE + Duration::from_millis(30));
    world.click_latest(1, 0, 40);
    let reqs = world.stub().wait_requests(2, FRAME_WAIT);
    assert!(reqs[1].contains("start"), "following click must start, got {}", reqs[1]);
    assert!(!reqs[1].contains("stop"), "following click must not stop: {}", reqs[1]);
    world.assert_wire();
}

#[test]
fn a4g_busy_rejection_is_notice_only() {
    let mut world = World::start();
    world.stub().set_reply(r#"{"ok":false,"error":"busy"}"#);
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(1, 0, 40);
    world.stub().wait_requests(1, FRAME_WAIT);
    world.click_latest(3, 0, 100);
    let expanded = world.wait_frame(FRAME_WAIT, |f| {
        f.min_width.is_none() && f.plain.contains("busy") && f.full_text.contains(ORANGE)
    });
    if !expanded.plain.starts_with(GLYPH_IDLE) {
        dump_fail("glyph unchanged", GLYPH_IDLE, &expanded.plain, &world.frames());
    }
    if expanded.full_text.contains(RED) && !expanded.plain.contains("busy") {
        dump_fail("must not become error state", "idle glyph, orange notice", &expanded.full_text, &world.frames());
    }
    // Glyph color stays blue, not the error red. "busy" is orange, not a red error glyph.
    if !expanded.full_text.contains(BLUE) {
        dump_fail("idle glyph color", BLUE, &expanded.full_text, &world.frames());
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(world.stub().requests().len(), 1);
    world.assert_wire();
}

// --- A5 -------------------------------------------------------------------

#[test]
fn a5_expanded_never_commands() {
    let mut world = World::start();
    world.stub().set_withhold();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(3, 0, 100);
    let expanded = world.wait_frame(FRAME_WAIT, |f| f.min_width.is_none() && f.plain.contains('→'));
    world.assert_frame_contract(&expanded, true);
    // Hit several regions plus the live edge. Expanded button 1 never commands.
    // Width 150; exact cell is irrelevant to the "no request" claim, but cover
    // left edge, mid row, and relative_x == width (last cell).
    for rx in [0, 10, 40, 80, 120, 149, 150] {
        world.click_latest(1, rx, 150);
        std::thread::sleep(Duration::from_millis(15));
    }
    world.click_latest(4, 0, 150);
    world.click_latest(5, 0, 150);
    std::thread::sleep(Duration::from_millis(40));
    assert!(world.stub().requests().is_empty(), "expanded clicks must not command: {:?}", world.stub().requests());

    let mark = world.frame_len();
    world.click_latest(2, 0, 150);
    let collapsed = world.wait_after(mark, FRAME_WAIT, |f| f.min_width.is_some());
    if collapsed.plain != GLYPH_IDLE {
        dump_fail("button 2 collapses", GLYPH_IDLE, &collapsed.plain, &world.frames());
    }
    let records = world.wait_launcher(FRAME_WAIT);
    assert_eq!(records.len(), 1, "configurator spawned once, got {records:?}");
    let cfg = world.config.join("config.toml");
    assert_eq!(
        records[0],
        vec![
            "--config".to_string(),
            cfg.display().to_string(),
            "configure".to_string(),
        ],
        "argv mismatch: {:?}",
        records[0]
    );
    std::thread::sleep(Duration::from_millis(40));
    assert!(world.stub().requests().is_empty(), "button 2 must not command");
    world.assert_wire();

    // Fresh process: expand, collapse, button 1 starts.
    let mut fresh = World::start();
    fresh.stub().set_withhold();
    push_and_wait(&fresh, &snap("idle"), GLYPH_IDLE);
    fresh.click_latest(3, 0, 100);
    fresh.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    let mark = fresh.frame_len();
    fresh.click_latest(3, 0, 100);
    fresh.wait_after(mark, FRAME_WAIT, |f| f.min_width.is_some() && f.plain == GLYPH_IDLE);
    fresh.click_latest(1, 0, 40);
    let reqs = fresh.stub().wait_requests(1, FRAME_WAIT);
    assert!(reqs[0].contains("start"), "legitimate start, got {}", reqs[0]);
    fresh.assert_wire();
}

// --- A6 -------------------------------------------------------------------

#[test]
fn a6_layout_and_side_selection() {
    // Literal cell count: 2+1+13+4+3 = 23. Width 230 so pitch is 10.
    const WIDTH: i32 = 230;
    const EP: &str = "◯ [cleanup-xai] →  S ";
    const EM: &str = "◯  cleanup-xai  → [S]";
    assert_eq!(hand_cells(EP), 23, "hand check of the literal 23");
    assert_eq!(hand_cells(EM), 23, "hand check of the literal 23");

    let mut world = World::start();
    fs::create_dir_all(world.config.join("presets/cleanup-xai")).unwrap();
    executable(
        &world.config.join("input-methods/ctrl-shift-v"),
        "#!/bin/sh\nexit 0\n",
    );
    link_rel(&world.config, "active-preset", "presets/cleanup-xai");
    link_rel(&world.config, "active-input-method", "input-methods/ctrl-shift-v");

    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(3, 0, WIDTH);
    let ep = world.wait_plain(EP, FRAME_WAIT);
    world.assert_frame_contract(&ep, true);
    assert_eq!(hand_cells(&ep.plain), 23);

    // relative_x = width-1 → last cell → method side.
    // (229 * 23) / 230 = 22, the last of 23 cells.
    world.click_latest(1, WIDTH - 1, WIDTH);
    let em = world.wait_plain(EM, FRAME_WAIT);
    assert_eq!(hand_cells(&em.plain), 23, "cell counts must be equal (literal 23)");
    assert_eq!(hand_cells(&ep.plain), hand_cells(&em.plain));

    // Arrow cells are 16..=19. Cell 17 → relative_x 170.
    // 170 * 23 / 230 = 17.
    world.click_latest(1, 170, WIDTH);
    std::thread::sleep(Duration::from_millis(80));
    let after_arrow = world.latest();
    if after_arrow.plain != EM {
        dump_fail("arrow click unchanged", EM, &after_arrow.plain, &world.frames());
    }

    // No geometry: side unchanged.
    world.click_latest(1, 0, 0);
    world.click_latest(1, -1, WIDTH);
    world.click_latest(1, WIDTH + 1, WIDTH);
    std::thread::sleep(Duration::from_millis(80));
    if world.latest().plain != EM {
        dump_fail("no-geometry unchanged", EM, &world.latest().plain, &world.frames());
    }

    // Back to preset side, then relative_x == width selects method (last cell).
    world.click_latest(1, 0, WIDTH);
    world.wait_plain(EP, FRAME_WAIT);
    world.click_latest(1, WIDTH, WIDTH);
    let edged = world.wait_plain(EM, FRAME_WAIT);
    assert_eq!(edged.plain, EM);
    assert!(world.stub().requests().is_empty(), "layout clicks must not command");
    world.assert_wire();
}

#[test]
fn a6_nogeometry_touches_timer() {
    const WIDTH: i32 = 230;
    const EP: &str = "◯ [cleanup-xai] →  S ";
    let mut world = World::start();
    fs::create_dir_all(world.config.join("presets/cleanup-xai")).unwrap();
    executable(&world.config.join("input-methods/ctrl-shift-v"), "#!/bin/sh\nexit 0\n");
    link_rel(&world.config, "active-preset", "presets/cleanup-xai");
    link_rel(&world.config, "active-input-method", "input-methods/ctrl-shift-v");
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);

    let t0 = Instant::now();
    world.click_latest(3, 0, WIDTH);
    world.wait_plain(EP, FRAME_WAIT);
    sleep_until(t0 + Duration::from_millis(200));
    // Three no-geometry clicks. Each must touch; one is enough to extend.
    world.click_latest(1, 0, 0);
    world.click_latest(1, -1, WIDTH);
    world.click_latest(1, WIDTH + 1, WIDTH);
    sleep_until(t0 + Duration::from_millis(420));
    let mid = world.latest();
    if mid.plain != EP || mid.min_width.is_some() {
        dump_fail(
            "timer touched: still expanded at 420ms (original hold is 300ms)",
            EP,
            &format!("plain={} min_width={:?}", mid.plain, mid.min_width),
            &world.frames(),
        );
    }
    let mark = world.frame_len();
    sleep_until(t0 + Duration::from_millis(750));
    let end = world.wait_after(mark, Duration::from_millis(200), |f| f.min_width.is_some());
    if end.plain != GLYPH_IDLE {
        dump_fail("eventual collapse", GLYPH_IDLE, &end.plain, &world.frames());
    }
}

// --- A7 -------------------------------------------------------------------

fn plant_cycle_dirs(config: &std::path::Path) {
    fs::create_dir_all(config.join("presets/a")).unwrap();
    fs::create_dir_all(config.join("presets/b")).unwrap();
    fs::create_dir_all(config.join("presets/.hidden")).unwrap();
    let notes = config.join("presets/notes.txt");
    fs::write(&notes, "nope").unwrap();
    let mut perms = fs::metadata(&notes).unwrap().permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&notes, perms).unwrap();
    for name in ["ctrl-shift-v", "ctrl-v", "mycustom", "type"] {
        executable(&config.join("input-methods").join(name), "#!/bin/sh\nexit 0\n");
    }
    fs::write(config.join(".active-preset.tmp"), b"python-owned").unwrap();
}

fn read_link_rel(path: &std::path::Path) -> Option<String> {
    fs::read_link(path).ok().map(|p| p.to_string_lossy().into_owned())
}

#[test]
fn a7_cycling() {
    let mut world = World::start();
    plant_cycle_dirs(&world.config);
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(3, 0, 200);
    let expanded = world.wait_frame(FRAME_WAIT, |f| f.plain.contains("[raw]"));
    assert!(expanded.plain.contains('?'), "missing method renders ?: {}", expanded.plain);

    // Scroll down: raw → a → b → raw.
    world.click_latest(5, 0, 200);
    world.wait_plain_contains("[a]", FRAME_WAIT);
    assert_eq!(read_link_rel(&world.config.join("active-preset")).as_deref(), Some("presets/a"));

    world.click_latest(5, 0, 200);
    world.wait_plain_contains("[b]", FRAME_WAIT);
    let link = read_link_rel(&world.config.join("active-preset")).unwrap_or_default();
    assert_eq!(link, "presets/b", "live link must be the relative target presets/b, got {link}");
    assert!(std::path::Path::new(&link).is_relative());

    let mark = world.frame_len();
    world.click_latest(5, 0, 200);
    world.wait_after(mark, FRAME_WAIT, |f| f.plain.contains("[raw]"));
    assert!(
        fs::symlink_metadata(world.config.join("active-preset")).is_err(),
        "raw removes the link, still {:?}",
        read_link_rel(&world.config.join("active-preset"))
    );
    let leftovers: Vec<_> = dir_names(&world.config)
        .into_iter()
        .filter(|n| n.starts_with(".active-preset.gruve."))
        .collect();
    assert!(leftovers.is_empty(), "temp symlink left behind: {leftovers:?}");
    assert_eq!(
        fs::read(world.config.join(".active-preset.tmp")).unwrap(),
        b"python-owned",
        "Python .active-preset.tmp must be untouched"
    );

    // Scroll up reverses: raw → b → a → raw. Match frames after each click
    // so an earlier [b]/[a]/[raw] cannot satisfy the wait.
    let mark = world.frame_len();
    world.click_latest(4, 0, 200);
    world.wait_after(mark, FRAME_WAIT, |f| f.plain.contains("[b]"));
    assert_eq!(read_link_rel(&world.config.join("active-preset")).as_deref(), Some("presets/b"));
    let mark = world.frame_len();
    world.click_latest(4, 0, 200);
    world.wait_after(mark, FRAME_WAIT, |f| f.plain.contains("[a]"));
    let mark = world.frame_len();
    world.click_latest(4, 0, 200);
    world.wait_after(mark, FRAME_WAIT, |f| f.plain.contains("[raw]"));
    assert!(fs::symlink_metadata(world.config.join("active-preset")).is_err());

    // Method side. relative_x == width hits the last cell → method.
    world.click_latest(1, 200, 200);
    world.wait_frame(FRAME_WAIT, |f| f.plain.contains("[?]"));

    // Plant type, then scroll.
    link_rel(&world.config, "active-input-method", "input-methods/type");
    // Symlink re-read is 50 ms while expanded; also a click re-reads on cycle.
    world.click_latest(5, 0, 200);
    let down = world.wait_plain_contains("[S]", FRAME_WAIT);
    assert!(down.plain.contains("[S]"), "scroll down from type wraps to ctrl-shift-v: {}", down.plain);
    assert_eq!(
        read_link_rel(&world.config.join("active-input-method")).as_deref(),
        Some("input-methods/ctrl-shift-v")
    );

    // Fresh from type, scroll up → mycustom.
    let mut up = World::start();
    plant_cycle_dirs(&up.config);
    link_rel(&up.config, "active-input-method", "input-methods/type");
    push_and_wait(&up, &snap("idle"), GLYPH_IDLE);
    up.click_latest(3, 0, 200);
    up.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    up.click_latest(1, 200, 200);
    up.wait_plain_contains("[T]", FRAME_WAIT);
    up.click_latest(4, 0, 200);
    let got = up.wait_plain_contains("[mycustom]", FRAME_WAIT);
    assert!(got.plain.contains("[mycustom]"), "{}", got.plain);
    assert_eq!(
        read_link_rel(&up.config.join("active-input-method")).as_deref(),
        Some("input-methods/mycustom")
    );

    // Missing link, both directions → ctrl-shift-v (index 0).
    for button in [4, 5] {
        let mut missing = World::start();
        plant_cycle_dirs(&missing.config);
        push_and_wait(&missing, &snap("idle"), GLYPH_IDLE);
        missing.click_latest(3, 0, 200);
        missing.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
        missing.click_latest(1, 200, 200);
        missing.wait_plain_contains("[?]", FRAME_WAIT);
        missing.click_latest(button, 0, 200);
        missing.wait_plain_contains("[S]", FRAME_WAIT);
        assert_eq!(
            read_link_rel(&missing.config.join("active-input-method")).as_deref(),
            Some("input-methods/ctrl-shift-v")
        );
    }

    // Abbreviations render S C mycustom T across a walk. Start at ctrl-shift-v, scroll down.
    let mut abbr = World::start();
    plant_cycle_dirs(&abbr.config);
    link_rel(&abbr.config, "active-input-method", "input-methods/ctrl-shift-v");
    push_and_wait(&abbr, &snap("idle"), GLYPH_IDLE);
    abbr.click_latest(3, 0, 200);
    abbr.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    abbr.click_latest(1, 200, 200);
    abbr.wait_plain_contains("[S]", FRAME_WAIT);
    abbr.click_latest(5, 0, 200);
    abbr.wait_plain_contains("[C]", FRAME_WAIT);
    abbr.click_latest(5, 0, 200);
    abbr.wait_plain_contains("[mycustom]", FRAME_WAIT);
    abbr.click_latest(5, 0, 200);
    abbr.wait_plain_contains("[T]", FRAME_WAIT);

    // Collapsed scroll changes nothing on disk.
    let mut collapsed = World::start();
    plant_cycle_dirs(&collapsed.config);
    link_rel(&collapsed.config, "active-preset", "presets/a");
    link_rel(&collapsed.config, "active-input-method", "input-methods/type");
    push_and_wait(&collapsed, &snap("idle"), GLYPH_IDLE);
    collapsed.click_latest(4, 0, 40);
    collapsed.click_latest(5, 0, 40);
    std::thread::sleep(SETTLE);
    assert_eq!(read_link_rel(&collapsed.config.join("active-preset")).as_deref(), Some("presets/a"));
    assert_eq!(
        read_link_rel(&collapsed.config.join("active-input-method")).as_deref(),
        Some("input-methods/type")
    );
    assert_eq!(collapsed.latest().plain, GLYPH_IDLE);
}

#[test]
fn a7_interleave_pause_hook() {
    let pause_dir = std::env::temp_dir().join(format!(
        "gruve-block-pause-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&pause_dir).unwrap();
    let pause = pause_dir.join("paused");
    let mut world = World::start_with(WorldOpts {
        extra_env: vec![("GRUVE_BLOCK_TEST_PAUSE".into(), pause.display().to_string())],
        ..WorldOpts::default()
    });
    world.track_dir(pause_dir);
    plant_cycle_dirs(&world.config);
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(3, 0, 200);
    world.wait_plain_contains("[raw]", FRAME_WAIT);

    world.click_latest(5, 0, 200);
    let start = Instant::now();
    loop {
        if pause.is_file() {
            break;
        }
        if start.elapsed() > Duration::from_secs(2) {
            panic!(
                "GRUVE_BLOCK_TEST_PAUSE file was not published at {} — block must write the temp path there after symlink() and before rename(), and block until it is unlinked",
                pause.display()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let temp_path = fs::read_to_string(&pause).unwrap();
    let temp_path = temp_path.trim();
    assert!(
        std::path::Path::new(temp_path).exists(),
        "pause file should name the temp symlink, got {temp_path:?}"
    );
    assert!(
        temp_path.contains(".active-preset.gruve."),
        "temp name {temp_path}"
    );

    // Harness repoints the live link at a different target while the block is paused.
    link_rel(&world.config, "active-preset", "presets/b");
    fs::remove_file(&pause).unwrap();

    world.wait_plain_contains("[a]", FRAME_WAIT);
    let link = read_link_rel(&world.config.join("active-preset")).unwrap_or_default();
    assert_eq!(link, "presets/a", "rename must publish the block's own target, got {link}");
    let leftovers: Vec<_> = dir_names(&world.config)
        .into_iter()
        .filter(|n| n.starts_with(".active-preset.gruve."))
        .collect();
    assert!(leftovers.is_empty(), "temp remained: {leftovers:?}");
    assert_eq!(fs::read(world.config.join(".active-preset.tmp")).unwrap(), b"python-owned");
}

// --- A8 -------------------------------------------------------------------

#[test]
fn a8_collapse_timer() {
    let mut world = World::start();
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    let t0 = Instant::now();
    let mark = world.frame_len();
    world.click_latest(3, 0, 100);
    world.wait_after(mark, FRAME_WAIT, |f| f.min_width.is_none());
    sleep_until(t0 + Duration::from_millis(150));
    assert!(
        world.latest().min_width.is_none(),
        "collapsed too early: {}",
        world.latest().plain
    );
    let collapsed = world.wait_after(mark, HOLD + Duration::from_millis(200), |f| f.min_width.is_some());
    assert_eq!(collapsed.plain, GLYPH_IDLE);
    assert!(
        t0.elapsed() + Duration::from_millis(30) >= HOLD,
        "collapsed before the scaled 6s: {:?}",
        t0.elapsed()
    );

    // A click at 5 s scaled (250 ms) extends.
    let mut ext = World::start();
    push_and_wait(&ext, &snap("idle"), GLYPH_IDLE);
    let t0 = Instant::now();
    let mark = ext.frame_len();
    ext.click_latest(3, 0, 100);
    ext.wait_after(mark, FRAME_WAIT, |f| f.min_width.is_none());
    sleep_until(t0 + EXTEND_AT);
    ext.click_latest(1, 0, 100); // touch, stay expanded
    sleep_until(t0 + Duration::from_millis(400));
    assert!(
        ext.latest().min_width.is_none(),
        "click at 250ms should extend past 300ms; plain={}",
        ext.latest().plain
    );
    sleep_until(t0 + EXTEND_AT + HOLD + Duration::from_millis(200));
    let end = ext.wait_after(mark, Duration::from_millis(200), |f| f.min_width.is_some());
    assert_eq!(end.plain, GLYPH_IDLE);

    // Snapshots while expanded do not extend.
    let mut snap_world = World::start();
    push_and_wait(&snap_world, &snap("idle"), GLYPH_IDLE);
    let t0 = Instant::now();
    snap_world.click_latest(3, 0, 100);
    snap_world.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    sleep_until(t0 + Duration::from_millis(200));
    snap_world.stub().push_line(&snap("recording"));
    snap_world.wait_frame(FRAME_WAIT, |f| f.plain.contains(GLYPH_ACTIVE));
    sleep_until(t0 + Duration::from_millis(420));
    let mid = snap_world.latest();
    if mid.min_width.is_none() {
        dump_fail(
            "snapshot must not extend the collapse timer",
            "collapsed by 420ms",
            &format!("still expanded: {}", mid.plain),
            &snap_world.frames(),
        );
    }

    // Button 2 collapses.
    let mut mid_click = World::start();
    push_and_wait(&mid_click, &snap("idle"), GLYPH_IDLE);
    let mark = mid_click.frame_len();
    mid_click.click_latest(3, 0, 100);
    mid_click.wait_after(mark, FRAME_WAIT, |f| f.min_width.is_none());
    let t = Instant::now();
    let mark = mid_click.frame_len();
    mid_click.click_latest(2, 0, 100);
    mid_click.wait_after(mark, Duration::from_millis(200), |f| f.min_width.is_some());
    assert!(t.elapsed() < HOLD, "button 2 should collapse immediately, took {:?}", t.elapsed());
}

#[test]
fn a8_reap_and_null_stdio() {
    let mut world = World::start_with(WorldOpts {
        extra_env: vec![("GRUVE_TEST_LAUNCHER_SLEEP".into(), "0.25".into())],
        ..WorldOpts::default()
    });
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(2, 0, 40);
    world.wait_launcher(FRAME_WAIT);
    let log = fs::read_to_string(&world.launcher_log).unwrap_or_default();
    let pid: u32 = log
        .lines()
        .find_map(|l| l.strip_prefix("PID "))
        .and_then(|s| s.parse().ok())
        .expect("launcher pid");
    // Wait until the sleeper has exited.
    let start = Instant::now();
    while proc_state(pid).is_some() && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    // Wake the loop so try_wait runs (poll may be idle).
    world.stub().push_line(&snap("idle"));
    std::thread::sleep(Duration::from_millis(100));
    if proc_state(pid) == Some('Z') {
        panic!("configurator pid {pid} is a zombie");
    }
    let zombies: Vec<_> = task_states(world.pid()).into_iter().filter(|s| *s == 'Z').collect();
    assert!(zombies.is_empty(), "zombie in /proc/{}/task", world.pid());

    // Noise on the child's stdio must not corrupt the JSON stream, and stdin
    // must not be stolen (a later click still expands).
    let mut noisy = World::start_with(WorldOpts {
        extra_env: vec![("GRUVE_TEST_LAUNCHER_NOISE".into(), "1".into())],
        ..WorldOpts::default()
    });
    push_and_wait(&noisy, &snap("idle"), GLYPH_IDLE);
    noisy.click_latest(2, 0, 40);
    noisy.wait_launcher(FRAME_WAIT);
    // Click while the launcher's read -t 0.3 is still in flight. If stdin was
    // not nulled, the script consumes this line and the block never expands.
    noisy.click_latest(3, 0, 100);
    let expanded = noisy.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    assert!(!expanded.raw_line.contains("NOISE"), "stdio leaked: {}", expanded.raw_line);
    noisy.assert_no_bad_lines();
    noisy.stub().push_line(&snap("recording"));
    let frame = noisy.wait_plain(GLYPH_ACTIVE, FRAME_WAIT);
    assert!(!frame.raw_line.contains("NOISE"), "stdio leaked into the JSON stream: {}", frame.raw_line);
    noisy.assert_no_bad_lines();
}

// --- A9 -------------------------------------------------------------------

#[test]
fn a9_frame_generation() {
    // (a) expanded frame, click carrying the previous collapsed idle instance → dropped.
    let mut world = World::start();
    world.stub().set_withhold();
    let collapsed = push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    let old = collapsed.instance.clone().expect("instance");
    world.click_latest(3, 0, 100);
    let expanded = world.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    assert_ne!(expanded.instance.as_deref(), Some(old.as_str()), "expand must bump instance");
    world.click(1, 0, 40, ClickInstance::Value(old));
    settle_no_request(&world, 0, "stale collapsed instance must not start");

    // (b) recording shown, idle published, click carries the recording instance → dropped.
    let mut b = World::start();
    b.stub().set_withhold();
    let rec = push_and_wait(&b, &snap("recording"), GLYPH_ACTIVE);
    let rec_inst = rec.instance.clone().unwrap();
    b.stub().push_line(&snap("idle"));
    let idle = b.wait_plain(GLYPH_IDLE, FRAME_WAIT);
    assert_ne!(idle.instance.as_deref(), Some(rec_inst.as_str()), "idle must bump instance");
    b.click(1, 0, 40, ClickInstance::Value(rec_inst));
    settle_no_request(&b, 0, "stale recording instance must not start or stop");

    // (c) click written before the idle line, both readable in one wake → one stop.
    // SIGSTOP makes the coalescing deterministic.
    let mut c = World::start();
    c.stub().set_withhold();
    let rec = push_and_wait(&c, &snap("recording"), GLYPH_ACTIVE);
    let rec_inst = rec.instance.clone().unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let pid = c.pid();
    assert!(Command::new("kill").args(["-STOP", &pid.to_string()]).status().unwrap().success());
    c.click(1, 0, 40, ClickInstance::Value(rec_inst));
    c.stub().push_line(&snap("idle"));
    assert!(Command::new("kill").args(["-CONT", &pid.to_string()]).status().unwrap().success());
    let reqs = c.stub().wait_requests(1, FRAME_WAIT);
    assert!(reqs[0].contains("stop"), "same-wake click must stop, got {}", reqs[0]);
    std::thread::sleep(SETTLE);
    assert_eq!(c.stub().requests().len(), 1, "exactly one stop: {:?}", c.stub().requests());
    assert!(!request_commands(&c).iter().any(|c| c == "start"));

    // (d) no instance → dropped.
    let mut d = World::start();
    d.stub().set_withhold();
    push_and_wait(&d, &snap("idle"), GLYPH_IDLE);
    d.click(1, 0, 40, ClickInstance::Omit);
    settle_no_request(&d, 0, "missing instance dropped");

    // (e) color-only keeps instance; idle → recording bumps; expanding bumps.
    let mut e = World::start();
    let idle = push_and_wait(&e, &snap("idle"), GLYPH_IDLE);
    let kept = idle.instance.clone().unwrap();
    e.stub().push_line(&snap_error("idle", "warn"));
    let warned = e.wait_frame(FRAME_WAIT, |f| f.full_text.contains(YELLOW));
    assert_eq!(warned.instance.as_deref(), Some(kept.as_str()), "color-only must keep instance");
    assert_eq!(warned.plain, GLYPH_IDLE);
    e.stub().push_line(&snap("recording"));
    let rec = e.wait_plain(GLYPH_ACTIVE, FRAME_WAIT);
    assert_ne!(rec.instance.as_deref(), Some(kept.as_str()), "idle → recording must bump instance");
    let before_expand = rec.instance.clone().unwrap();
    e.click_latest(3, 0, 100);
    let expanded = e.wait_frame(FRAME_WAIT, |f| f.min_width.is_none());
    assert_ne!(
        expanded.instance.as_deref(),
        Some(before_expand.as_str()),
        "expanding must bump instance"
    );
}

// --- A10 ------------------------------------------------------------------

#[test]
fn a10_reconnect_and_robustness() {
    let mut world = World::start();
    let idle = push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    assert_eq!(idle.plain, GLYPH_IDLE);
    let subs_before = world.stub().subscribe_count();
    let mark = world.frame_len();
    world.stub().close_subs();
    world.wait_after(mark, FRAME_WAIT, |f| f.plain == GLYPH_OFFLINE);
    world.click_latest(1, 0, 40);
    // Outage click is nothing then and nothing after reconnect.
    world.stub().wait_subscribes(subs_before + 1, FRAME_WAIT);
    world.stub().push_line(&snap("recording"));
    let back = world.wait_plain(GLYPH_ACTIVE, Duration::from_millis(1000));
    assert_eq!(back.plain, GLYPH_ACTIVE);
    std::thread::sleep(SETTLE);
    assert!(
        world.stub().requests().is_empty(),
        "outage click must not replay: {:?}",
        world.stub().requests()
    );
    let after = world.stub().all_lines();
    let reconnect_cmds: Vec<_> = after
        .iter()
        .skip(subs_before)
        .map(|l| serde_json::from_str::<serde_json::Value>(l).ok().and_then(|v| v.get("command").and_then(|c| c.as_str()).map(|s| s.to_string())).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        reconnect_cmds.iter().all(|c| c == "subscribe"),
        "only subscribe after reconnect, got {reconnect_cmds:?}"
    );

    // Malformed → ?, button 1 nothing, next valid line restores idle.
    let mut bad = World::start();
    bad.stub().set_withhold();
    bad.stub().wait_subscribes(1, FRAME_WAIT);
    bad.stub().push_line("not-json");
    bad.wait_plain(GLYPH_UNKNOWN, FRAME_WAIT);
    bad.click_latest(1, 0, 40);
    settle_no_request(&bad, 0, "malformed suppresses button 1");
    bad.stub().push_line(&snap("idle"));
    bad.wait_plain(GLYPH_IDLE, FRAME_WAIT);
    bad.click_latest(1, 0, 40);
    let reqs = bad.stub().wait_requests(1, FRAME_WAIT);
    assert!(reqs[0].contains("start"), "{}", reqs[0]);

    // 1 MiB junk → offline, reconnect, next valid snapshot on the new connection.
    let junk = World::start();
    junk.stub().wait_subscribes(1, FRAME_WAIT);
    junk.stub().push_line(&snap("idle"));
    junk.wait_plain(GLYPH_IDLE, FRAME_WAIT);
    let n = junk.stub().subscribe_count();
    let mark = junk.frame_len();
    let blob = vec![b'x'; 1024 * 1024];
    junk.stub().push_raw(&blob);
    junk.wait_after(mark, FRAME_WAIT, |f| f.plain == GLYPH_OFFLINE);
    junk.stub().wait_subscribes(n + 1, FRAME_WAIT);
    junk.stub().push_line(&snap("recording"));
    junk.wait_plain(GLYPH_ACTIVE, FRAME_WAIT);

    // Accepts and never sends the first snapshot → offline after the scaled 2 s,
    // observable as the connection being dropped and a resubscribe.
    let silent = World::start();
    let t0 = Instant::now();
    silent.stub().wait_subscribes(1, FRAME_WAIT);
    silent.stub().wait_subscribes(2, Duration::from_millis(1000));
    assert!(
        t0.elapsed() < Duration::from_millis(1000),
        "resubscribe took {:?}; scaled connect+reconnect is ~175ms",
        t0.elapsed()
    );
    let frame = silent.wait_plain(GLYPH_OFFLINE, FRAME_WAIT);
    assert_eq!(frame.plain, GLYPH_OFFLINE);

    // stdin EOF → exit 0.
    let mut eof = World::start();
    eof.wait_frame(FRAME_WAIT, |_| true);
    eof.close_stdin();
    let status = eof.wait_exit(FRAME_WAIT);
    assert!(status.success(), "stdin EOF should exit 0, got {status}");

    // Snapshot split across two writes → one idle frame, no unknown frame.
    let split = World::start();
    split.stub().wait_subscribes(1, FRAME_WAIT);
    split.stub().push_raw(br#"{"state":"id"#);
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        !split.frames().iter().any(|f| f.plain == GLYPH_UNKNOWN),
        "partial line must not render: {}",
        format_frames(&split.frames())
    );
    split.stub().push_raw(br#"le","pipeline":"idle","error":null}"#);
    split.stub().push_raw(b"\n");
    split.wait_plain(GLYPH_IDLE, FRAME_WAIT);
    let idles = split.frames().iter().filter(|f| f.plain == GLYPH_IDLE).count();
    assert_eq!(idles, 1, "split snapshot must produce one frame: {}", format_frames(&split.frames()));
    assert!(!split.frames().iter().any(|f| f.plain == GLYPH_UNKNOWN));
}

// --- A11 ------------------------------------------------------------------

#[test]
fn a11_dedup() {
    let world = World::start();
    world.stub().wait_subscribes(1, FRAME_WAIT);
    world.stub().push_line(&snap("idle"));
    world.wait_plain(GLYPH_IDLE, FRAME_WAIT);
    world.stub().push_line(&snap("idle"));
    std::thread::sleep(SETTLE);
    let idles = world.frames().iter().filter(|f| f.plain == GLYPH_IDLE).count();
    assert_eq!(
        idles,
        1,
        "identical snapshots must produce one line: {}",
        format_frames(&world.frames())
    );
    world.assert_no_bad_lines();
}

// --- A12 ------------------------------------------------------------------

#[test]
fn a12_print_paths_and_isolation() {
    let home = std::env::temp_dir().join(format!("gruve-block-home-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let uid = uid();
    let (status, out, err) = run_print_paths(&home, &[]);
    assert!(status.success(), "print-paths exit {status} stderr={err}");
    let lines: Vec<_> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines,
        vec![
            format!("/tmp/bnuuy-stt-{uid}/bnuuy-stt"),
            format!("{}/.config/bnuuy-stt", home.display()),
        ],
        "unset XDG print-paths:\n{out}"
    );
    assert!(home_entries(&home).is_empty(), "print-paths wrote under HOME: {:?}", home_entries(&home));

    let xdg_runtime = std::env::temp_dir().join(format!("gruve-block-xdg-rt-{}", std::process::id()));
    let xdg_config = std::env::temp_dir().join(format!("gruve-block-xdg-cfg-{}", std::process::id()));
    fs::create_dir_all(&xdg_runtime).unwrap();
    fs::create_dir_all(&xdg_config).unwrap();
    let (status, out, err) = run_print_paths(
        &home,
        &[
            ("XDG_RUNTIME_DIR", xdg_runtime.to_str().unwrap()),
            ("XDG_CONFIG_HOME", xdg_config.to_str().unwrap()),
        ],
    );
    assert!(status.success(), "print-paths xdg exit {status} stderr={err}");
    let lines: Vec<_> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines,
        vec![
            format!("{}/bnuuy-stt", xdg_runtime.display()),
            format!("{}/bnuuy-stt", xdg_config.display()),
        ],
        "set XDG print-paths:\n{out}"
    );

    // Non-default --config-dir: configurator argv names that dir; HOME stays empty.
    let mut world = World::start();
    assert!(
        home_entries(&world.home).is_empty(),
        "HOME not empty at start: {:?}",
        home_entries(&world.home)
    );
    push_and_wait(&world, &snap("idle"), GLYPH_IDLE);
    world.click_latest(2, 0, 40);
    let records = world.wait_launcher(FRAME_WAIT);
    let cfg = world.config.join("config.toml");
    assert_eq!(
        records[0],
        vec!["--config".into(), cfg.display().to_string(), "configure".into()]
    );
    assert!(
        !world.home.join(".config").exists(),
        "default config dir was created under HOME"
    );
    assert!(
        home_entries(&world.home).is_empty(),
        "HOME was written: {:?}",
        home_entries(&world.home)
    );
    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(&xdg_runtime);
    let _ = fs::remove_dir_all(&xdg_config);
}

fn run_print_paths(home: &std::path::Path, extra: &[(&str, &str)]) -> (std::process::ExitStatus, String, String) {
    let bin = env!("CARGO_BIN_EXE_gruve-block");
    let mut cmd = Command::new(bin);
    cmd.env_clear();
    cmd.env("HOME", home);
    cmd.env("PATH", "/usr/bin:/bin");
    cmd.env("LANG", "C");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.arg("--print-paths");
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut out = String::new();
            let mut err = String::new();
            let _ = child.stdout.take().unwrap().read_to_string(&mut out);
            let _ = child.stderr.take().unwrap().read_to_string(&mut err);
            return (status, out, err);
        }
        if start.elapsed() > Duration::from_secs(3) {
            let _ = child.kill();
            panic!("--print-paths hung (must not connect)");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}


