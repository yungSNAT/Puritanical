#![forbid(unsafe_code)]

//! Shell quoting, env assignment prefixes, runner stripping and command segment splitting.

use super::{is_ascii_alpha, is_ascii_digit};
use crate::utf16::{
    eq_ascii, is_js_whitespace, is_js_word, is_line_terminator, starts_with_ascii, trim,
};

const RUNNERS: &[&[u8]] = &[
    b"sudo", b"time", b"command", b"exec", b"nice", b"env", b"npx", b"bunx", b"pnpx",
];

const EXACT_PAIRS: &[&[u8]] = &[
    b"pnpm exec",
    b"pnpm dlx",
    b"yarn exec",
    b"yarn dlx",
    b"uv run",
    b"poetry run",
    b"pipenv run",
    b"rye run",
    b"hatch run",
    b"pdm run",
    b"bundle exec",
    b"py -m",
];

fn consume_env_value(s: &[u16], mut i: usize) -> usize {
    while i < s.len() {
        if let Some(next) = match_single_quote(s, i).or_else(|| match_double_quote(s, i)) {
            i = next;
        } else if !is_js_whitespace(s[i]) && !matches!(s[i], 0x0027 | 0x0022 | 0x005C) {
            i += 1;
            while i < s.len()
                && !is_js_whitespace(s[i])
                && !matches!(s[i], 0x0027 | 0x0022 | 0x005C)
            {
                i += 1;
            }
        } else {
            break;
        }
    }
    i
}

#[must_use]
pub fn env_assign_end(s: &[u16], start: usize) -> usize {
    let mut end = start.min(s.len());
    loop {
        let Some(&first) = s.get(end) else {
            return end;
        };
        if !(is_ascii_alpha(first) || first == u16::from(b'_')) {
            return end;
        }
        let mut i = end + 1;
        while i < s.len() && is_js_word(s[i]) {
            i += 1;
        }
        if s.get(i) != Some(&u16::from(b'=')) {
            return end;
        }
        let after_val = consume_env_value(s, i + 1);
        let mut ws = after_val;
        while ws < s.len() && is_js_whitespace(s[ws]) {
            ws += 1;
        }
        if ws == after_val {
            return end;
        }
        end = ws;
    }
}

fn match_single_quote(s: &[u16], start: usize) -> Option<usize> {
    if s.get(start) != Some(&0x0027) {
        return None;
    }
    let rel = s[start + 1..].iter().position(|&u| u == 0x0027)?;
    Some(start + 2 + rel)
}

fn match_double_quote(s: &[u16], start: usize) -> Option<usize> {
    if s.get(start) != Some(&0x0022) {
        return None;
    }
    let mut i = start + 1;
    while i < s.len() {
        if s[i] == 0x0022 {
            return Some(i + 1);
        }
        if s[i] == 0x005C {
            let next = *s.get(i + 1)?;
            if is_line_terminator(next) {
                return None;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}

/// Replaces `'[^']*'` and `"(?:\\.|[^"\\])*"` with `""`.
#[must_use]
pub fn strip_quotes(cmd: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(cmd.len());
    let mut i = 0;
    while i < cmd.len() {
        if let Some(end) = match_single_quote(cmd, i).or_else(|| match_double_quote(cmd, i)) {
            out.push(0x0022);
            out.push(0x0022);
            i = end;
        } else {
            out.push(cmd[i]);
            i += 1;
        }
    }
    out
}

pub(crate) fn split_ws(s: &[u16]) -> Vec<&[u16]> {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < s.len() {
        while i < s.len() && is_js_whitespace(s[i]) {
            i += 1;
        }
        if i >= s.len() {
            break;
        }
        let start = i;
        while i < s.len() && !is_js_whitespace(s[i]) {
            i += 1;
        }
        parts.push(&s[start..i]);
    }
    parts
}

fn skip_required_ws(s: &[u16], mut k: usize) -> Option<usize> {
    if k >= s.len() || !is_js_whitespace(s[k]) {
        return None;
    }
    while k < s.len() && is_js_whitespace(s[k]) {
        k += 1;
    }
    Some(k)
}

fn runner_pair_end(s: &[u16]) -> Option<usize> {
    for &p in EXACT_PAIRS {
        if starts_with_ascii(s, p)
            && let Some(next) = skip_required_ws(s, p.len())
        {
            return Some(next);
        }
    }
    if starts_with_ascii(s, b"python") {
        let mut k = 6;
        while k < s.len() && (is_ascii_digit(s[k]) || s[k] == u16::from(b'.')) {
            k += 1;
        }
        if starts_with_ascii(&s[k..], b" -m") {
            return skip_required_ws(s, k + 3);
        }
    }
    None
}

pub(crate) fn strip_runners(segment: &[u16]) -> Vec<u16> {
    let mut s = segment[env_assign_end(segment, 0)..].to_vec();
    loop {
        let first_word: Vec<u16> = s
            .iter()
            .copied()
            .take_while(|&u| !is_js_whitespace(u))
            .collect();
        if RUNNERS.iter().any(|&r| eq_ascii(&first_word, r)) {
            let trimmed = trim(&s[first_word.len()..]).to_vec();
            s = trimmed[env_assign_end(&trimmed, 0)..].to_vec();
            continue;
        }
        if let Some(pair_end) = runner_pair_end(&s) {
            s = s[pair_end..].to_vec();
            continue;
        }
        return s;
    }
}

/// Splits stripped command on `&&`, `||`, `;`, `\n`.
pub(crate) fn split_segments(s: &[u16]) -> Vec<&[u16]> {
    let mut segs = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        if (s[i] == u16::from(b'&') && s.get(i + 1) == Some(&u16::from(b'&')))
            || (s[i] == u16::from(b'|') && s.get(i + 1) == Some(&u16::from(b'|')))
        {
            segs.push(&s[start..i]);
            i += 2;
            start = i;
        } else if matches!(s[i], 0x003B | 0x000A) {
            segs.push(&s[start..i]);
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    segs.push(&s[start..]);
    segs
}
