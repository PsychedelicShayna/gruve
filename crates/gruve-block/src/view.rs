//! Glyph table and the expanded delivery row. Wide set is block-owned.

use std::ffi::OsStr;

use bnuuy_rustblocks::colors::{self, Color};
use bnuuy_rustblocks::pango::Span;
use bnuuy_rustblocks::text::{self, sanitize_ascii};
use bnuuy_rustblocks::widget::Row;

use crate::stt::{Intent, Phase};

pub const WIDE: &[char] = &['◯', '◉', '●', '→', '…'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Glyph,
    Gap,
    Preset,
    Arrow,
    Method,
    Notice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Preset,
    Method,
}

pub struct Look {
    pub glyph: &'static str,
    pub color: Color,
    pub intent: Intent,
}

pub fn look(phase: Phase) -> Look {
    let intent = phase.intent();
    let (glyph, color) = match phase {
        Phase::Offline => ("◌", colors::FG_DIM),
        Phase::Idle => ("◯", colors::BLUE),
        Phase::IdleWarned => ("◯", colors::YELLOW),
        Phase::Starting => ("◉", colors::BLUE),
        Phase::Recording | Phase::Connected => ("●", colors::BLUE),
        Phase::Transcribing | Phase::Delivering => ("●", colors::FG_DIM),
        Phase::Error => ("◯", colors::RED),
        Phase::Unknown => ("?", colors::ORANGE),
    };
    Look {
        glyph,
        color,
        intent,
    }
}

pub fn span(color: Color) -> Span {
    Span::default().foreground(colors::hex(color))
}

pub fn abbreviate_input(raw: &str) -> String {
    match raw {
        "ctrl-v" => "C".to_string(),
        "ctrl-shift-v" => "S".to_string(),
        "type" => "T".to_string(),
        other => sanitize_ascii(other),
    }
}

pub fn display_os(raw: &OsStr) -> String {
    sanitize_ascii(&raw.to_string_lossy())
}

/// Truncate to `max` cells. The ellipsis counts as 2.
pub fn truncate_cells(s: &str, max: usize) -> String {
    if text::cells(s, WIDE) <= max {
        return s.to_string();
    }
    let ellipsis = '…';
    let budget = max.saturating_sub(text::cell_width(ellipsis, WIDE));
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let width = text::cell_width(ch, WIDE);
        if used + width > budget {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push(ellipsis);
    out
}

pub enum NoticeKind {
    Error(String),
    Local(String),
    Pipeline(String),
}

pub fn build_row(
    phase: Phase,
    preset: &str,
    method: &str,
    side: Side,
    notice: Option<&NoticeKind>,
) -> Row<Tag> {
    let look = look(phase);
    let mut row = Row::new(WIDE);
    row.seg(Tag::Glyph).run(look.glyph, span(look.color));
    row.seg(Tag::Gap).run(" ", span(colors::FG_DIM));
    push_choice(&mut row, Tag::Preset, preset, side == Side::Preset);
    row.seg(Tag::Arrow).run(" → ", span(colors::FG_DIM));
    push_choice(&mut row, Tag::Method, method, side == Side::Method);
    if let Some(notice) = notice {
        let (text, color) = match notice {
            NoticeKind::Error(text) => (text, colors::RED),
            NoticeKind::Local(text) => (text, colors::ORANGE),
            NoticeKind::Pipeline(text) => (text, colors::FG_DIM),
        };
        let shown = truncate_cells(text, 40);
        row.seg(Tag::Notice)
            .run(" ", span(color))
            .run(&shown, span(color));
    }
    row
}

fn push_choice(row: &mut Row<Tag>, tag: Tag, name: &str, selected: bool) {
    if selected {
        row.seg(tag)
            .run("[", span(colors::AQUA))
            .run(name, span(colors::FG))
            .run("]", span(colors::AQUA));
    } else {
        row.seg(tag)
            .run(" ", span(colors::FG_DIM))
            .run(name, span(colors::FG_DIM))
            .run(" ", span(colors::FG_DIM));
    }
}

pub fn tag_name(tag: Tag) -> &'static str {
    match tag {
        Tag::Glyph => "Glyph",
        Tag::Gap => "Gap",
        Tag::Preset => "Preset",
        Tag::Arrow => "Arrow",
        Tag::Method => "Method",
        Tag::Notice => "Notice",
    }
}
