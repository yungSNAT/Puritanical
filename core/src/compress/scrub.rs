#![forbid(unsafe_code)]

//! Lossless/noise scrubbing tier for tool output compression.

use super::json::{minify_json, strip_ansi};
use super::salvage::has_raw_salvage;
use super::{NOISE_MIN, REPEAT_MIN, is_ascii_digit};
use crate::utf16::{
    is_js_whitespace, is_js_word, is_line_terminator, join_u16, push_ascii, push_usize, split_lf,
    starts_with_ascii, trim,
};

const NOISE_PREFIX_WORDS: &[&[u8]] = &[
    b"Downloading",
    b"Downloaded",
    b"Fetching",
    b"Resolving",
    b"Receiving objects",
    b"Compiling",
    b"Installing",
    b"Extracting",
    b"Unpacking",
    b"Progress",
];

fn trim_trailing_ws_per_line(s: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if matches!(s[i], 0x0020 | 0x0009) {
            let mut j = i;
            while j < s.len() && matches!(s[j], 0x0020 | 0x0009) {
                j += 1;
            }
            if j < s.len() && !is_line_terminator(s[j]) {
                out.extend_from_slice(&s[i..j]);
            }
            i = j;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// Strips `[ \t]+$` at every JS line boundary and collapses 3+ `\n` to `\n\n`.
fn normalise_whitespace_lines(s: &[u16]) -> Vec<u16> {
    let trimmed = trim_trailing_ws_per_line(s);
    let mut collapsed = Vec::with_capacity(trimmed.len());
    let mut nl_run = 0usize;
    for u in trimmed {
        if u == 0x000A {
            nl_run += 1;
            if nl_run <= 2 {
                collapsed.push(u);
            }
        } else {
            nl_run = 0;
            collapsed.push(u);
        }
    }
    collapsed
}

fn is_ok_num_prefix(after_ws: &[u16]) -> bool {
    if !starts_with_ascii(after_ws, b"ok ") {
        return false;
    }
    let mut d = 3;
    while d < after_ws.len() && is_ascii_digit(after_ws[d]) {
        d += 1;
    }
    d > 3 && (d == after_ws.len() || !is_js_word(after_ws[d]))
}

fn has_noise_prefix(after_ws: &[u16]) -> bool {
    let Some(&first) = after_ws.first() else {
        return false;
    };
    if matches!(first, 0x2713 | 0x2714 | 0x221A)
        && after_ws.get(1).is_some_and(|&c| is_js_whitespace(c))
    {
        return true;
    }
    if (starts_with_ascii(after_ws, b"PASS")
        && after_ws.get(4).is_some_and(|&c| is_js_whitespace(c)))
        || starts_with_ascii(after_ws, b"[PASS]")
        || is_ok_num_prefix(after_ws)
    {
        return true;
    }
    NOISE_PREFIX_WORDS.iter().any(|&w| {
        starts_with_ascii(after_ws, w)
            && (after_ws.len() == w.len() || !is_js_word(after_ws[w.len()]))
    })
}

fn has_passed_anywhere(line: &[u16]) -> bool {
    (1..line.len()).any(|k| {
        is_js_whitespace(line[k - 1])
            && starts_with_ascii(&line[k..], b"PASSED")
            && (k + 6 == line.len() || !is_js_word(line[k + 6]))
    })
}

#[must_use]
fn is_noise_line(line: &[u16]) -> bool {
    let mut i = 0;
    while i < line.len() && is_js_whitespace(line[i]) {
        i += 1;
    }
    if has_noise_prefix(&line[i..]) || has_passed_anywhere(line) {
        return !has_raw_salvage(line);
    }
    false
}

fn collapse_repeats(lines: &[&[u16]]) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let mut j = i;
        while j < lines.len() && lines[j] == lines[i] {
            j += 1;
        }
        let n = j - i;
        if n >= REPEAT_MIN && !trim(lines[i]).is_empty() {
            out.push(lines[i].to_vec());
            let mut note = Vec::new();
            push_ascii(&mut note, "... [puritanical: line repeated ");
            push_usize(&mut note, n);
            note.push(0x00D7);
            push_ascii(&mut note, "] ...");
            out.push(note);
        } else {
            out.extend((0..n).map(|_| lines[i].to_vec()));
        }
        i = j;
    }
    out
}

fn collapse_noise_runs(lines: &[Vec<u16>]) -> Vec<u16> {
    let mut markers: Vec<Vec<u16>> = Vec::new();
    let mut entries: Vec<(bool, usize)> = Vec::with_capacity(lines.len());
    let mut idx = 0;
    while idx < lines.len() {
        let mut j = idx;
        while j < lines.len() && is_noise_line(&lines[j]) {
            j += 1;
        }
        if j - idx >= NOISE_MIN {
            entries.push((false, idx));
            entries.push((false, idx + 1));
            let mut note = Vec::new();
            push_ascii(&mut note, "... [puritanical: ");
            push_usize(&mut note, j - idx - 3);
            push_ascii(&mut note, " more passing/progress lines like these] ...");
            entries.push((true, markers.len()));
            markers.push(note);
            entries.push((false, j - 1));
        } else {
            j = j.max(idx + 1);
            entries.extend((idx..j).map(|pos| (false, pos)));
        }
        idx = j;
    }
    let slices: Vec<&[u16]> = entries
        .iter()
        .map(|&(is_m, pos)| {
            if is_m {
                markers[pos].as_slice()
            } else {
                lines[pos].as_slice()
            }
        })
        .collect();
    join_u16(&slices, 0x000A)
}

#[must_use]
pub fn scrub(text: &[u16]) -> Vec<u16> {
    let no_ansi = strip_ansi(text);
    let min = minify_json(&no_ansi);
    let source = min.as_deref().unwrap_or(&no_ansi);
    let normalised = normalise_whitespace_lines(source);
    let lines = split_lf(&normalised);
    let deduped = collapse_repeats(&lines);
    collapse_noise_runs(&deduped)
}
