#![forbid(unsafe_code)]

//! Error-like line salvaging from compressed tool output.

use super::{MAX_SALVAGE_LINE, is_ascii_alnum};
use crate::utf16::{find_ascii_ci, push_ascii, safe_slice, starts_with_ascii_ci};

const STATIC_KEYWORDS: &[&[u8]] = &[
    b"error",
    b"exception",
    b"traceback",
    b"panic",
    b"fatal",
    b"denied",
    b"refused",
    b"segfault",
    b"npe",
    b"undefined reference",
    b"cannot find",
    b"not found",
    b"warning",
];

fn has_word_right_boundary(s: &[u16], end: usize) -> bool {
    end >= s.len() || !is_ascii_alnum(s[end])
}

fn match_timed_out(s: &[u16], i: usize) -> Option<usize> {
    let rest = &s[i..];
    let t_len = if starts_with_ascii_ci(rest, b"timed") {
        5
    } else if starts_with_ascii_ci(rest, b"time") {
        4
    } else {
        return None;
    };
    let sep_len = usize::from(
        rest.get(t_len)
            .is_some_and(|&c| matches!(c, 0x0020 | 0x005F | 0x002D)),
    );
    let total = t_len + sep_len + 3;
    (starts_with_ascii_ci(&rest[t_len + sep_len..], b"out")
        && has_word_right_boundary(s, i + total))
    .then_some(total)
}

fn match_fail_word(s: &[u16], i: usize) -> Option<usize> {
    let rest = &s[i..];
    if !starts_with_ascii_ci(rest, b"fail") {
        return None;
    }
    for suf in [&b"ed"[..], b"ure", b"ing"] {
        if starts_with_ascii_ci(&rest[4..], suf) && has_word_right_boundary(s, i + 4 + suf.len()) {
            return Some(4 + suf.len());
        }
    }
    has_word_right_boundary(s, i + 4).then_some(4)
}

fn match_assert_word(s: &[u16], i: usize) -> Option<usize> {
    let rest = &s[i..];
    if !starts_with_ascii_ci(rest, b"assert") {
        return None;
    }
    if starts_with_ascii_ci(&rest[6..], b"ion") && has_word_right_boundary(s, i + 9) {
        return Some(9);
    }
    has_word_right_boundary(s, i + 6).then_some(6)
}

fn match_salvage_at(s: &[u16], i: usize) -> Option<usize> {
    if i > 0 && is_ascii_alnum(s[i - 1]) {
        return None;
    }
    let rest = &s[i..];
    if starts_with_ascii_ci(rest, b"err!") {
        return Some(4);
    }
    if let Some(len) = match_timed_out(s, i)
        .or_else(|| match_fail_word(s, i))
        .or_else(|| match_assert_word(s, i))
    {
        return Some(len);
    }
    STATIC_KEYWORDS
        .iter()
        .find(|&&kw| starts_with_ascii_ci(rest, kw) && has_word_right_boundary(s, i + kw.len()))
        .map(|kw| kw.len())
}

#[must_use]
pub fn find_salvage_match(s: &[u16]) -> Option<(usize, usize)> {
    (0..s.len()).find_map(|i| match_salvage_at(s, i).map(|len| (i, len)))
}

#[must_use]
pub fn has_raw_salvage(s: &[u16]) -> bool {
    find_salvage_match(s).is_some()
}

fn replace_error_like(line: &[u16]) -> Vec<u16> {
    if find_ascii_ci(line, b"error-like").is_none() {
        return line.to_vec();
    }
    let needle = b"error-like";
    let mut out = Vec::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        if starts_with_ascii_ci(&line[i..], needle) {
            push_ascii(&mut out, "diagnostic");
            i += needle.len();
        } else {
            out.push(line[i]);
            i += 1;
        }
    }
    out
}

/// Rescues error-like lines (capping at 300 UTF-16 code units centred on match).
#[must_use]
pub fn salvage_line(line: &[u16]) -> Option<Vec<u16>> {
    let scrubbed = replace_error_like(line);
    let (idx, len) = find_salvage_match(&scrubbed)?;
    if line.len() <= MAX_SALVAGE_LINE {
        return Some(line.to_vec());
    }
    let content_limit = MAX_SALVAGE_LINE - 2;
    let half = content_limit.saturating_sub(len) / 2;
    let raw_start = idx as isize - half as isize;
    let max_start = line.len().saturating_sub(content_limit) as isize;
    let start = raw_start.min(max_start).max(0) as usize;
    let end = start + content_limit;
    let mut out = Vec::with_capacity(MAX_SALVAGE_LINE);
    if start > 0 {
        out.push(0x2026);
    }
    out.extend_from_slice(safe_slice(line, start, end));
    if end < line.len() {
        out.push(0x2026);
    }
    Some(out)
}
