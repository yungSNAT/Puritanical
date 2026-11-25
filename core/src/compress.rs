#![forbid(unsafe_code)]

//! Port of pure output-compression functions from `hooks/puritanical-compress-output.js`:
//! `unsafeToolName`, `salvageLine`, `containsCredential`, `duplicateMarker`,
//! `scrub`, `minifyJson`, `compress`, `boundForOpencode`.

use crate::utf16::{
    find_ascii_ci, is_js_whitespace, is_js_word, is_line_terminator, join_u16, push_ascii,
    push_commas, push_usize, safe_prefix, safe_slice, safe_suffix, split_lf, starts_with_ascii,
    starts_with_ascii_ci, trim,
};

pub const DEFAULT_MAX_CHARS: usize = 8000;
pub const MAX_SALVAGED: usize = 12;
pub const MAX_SALVAGE_LINE: usize = 300;
pub const REPEAT_MIN: usize = 4;
pub const NOISE_MIN: usize = 6;
const AUTH_WINDOW: usize = 200;

const UNSAFE_TOOL_SUBSTRINGS: &[&[u8]] = &[
    b"read",
    b"view",
    b"file",
    b"edit",
    b"write",
    b"patch",
    b"replace",
    b"insert",
    b"lsp",
    b"symbol",
    b"definition",
    b"declaration",
    b"implementation",
    b"reference",
    b"hover",
    b"content",
    b"source",
    b"blob",
    b"search_code",
];

#[must_use]
pub fn unsafe_tool_name(name: &[u16]) -> bool {
    UNSAFE_TOOL_SUBSTRINGS
        .iter()
        .any(|needle| find_ascii_ci(name, needle).is_some())
}

#[must_use]
fn is_ascii_alnum(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0041..=0x005A | 0x0061..=0x007A)
}

#[must_use]
fn is_ascii_digit(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039)
}

#[must_use]
fn is_ascii_hex(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0041..=0x0046 | 0x0061..=0x0066)
}

#[must_use]
fn is_ascii_upper(u: u16) -> bool {
    matches!(u, 0x0041..=0x005A)
}

/// Strips CSI (`\x1b[...`) and OSC (`\x1b]...\x07|\x1b\\`) escapes matching `ANSI_RE`.
#[must_use]
pub fn strip_ansi(s: &[u16]) -> Vec<u16> {
    if !s.contains(&0x001B) {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == 0x001B
            && let Some(skip) = match_ansi_at(s, i)
        {
            i += skip;
            continue;
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

fn match_ansi_at(s: &[u16], i: usize) -> Option<usize> {
    let next = *s.get(i + 1)?;
    if next == u16::from(b'[') {
        let mut k = i + 2;
        while k < s.len() && matches!(s[k], 0x0030..=0x0039 | 0x003B | 0x003F) {
            k += 1;
        }
        while k < s.len() && (0x0020..=0x002F).contains(&s[k]) {
            k += 1;
        }
        if k < s.len() && (0x0040..=0x007E).contains(&s[k]) {
            return Some(k + 1 - i);
        }
        return None;
    }
    if next == u16::from(b']') {
        let mut k = i + 2;
        while k < s.len() && s[k] != 0x0007 && s[k] != 0x001B {
            k += 1;
        }
        if k < s.len() && s[k] == 0x0007 {
            return Some(k + 1 - i);
        }
        if k + 1 < s.len() && s[k] == 0x001B && s[k + 1] == u16::from(b'\\') {
            return Some(k + 2 - i);
        }
    }
    None
}

/// Validates RFC 8259 / V8 `JSON.parse` syntax without parsing numbers/objects.
#[must_use]
fn is_valid_json(s: &[u16]) -> bool {
    let mut pos = skip_json_ws(s, 0);
    if !consume_json_value(s, &mut pos) {
        return false;
    }
    pos = skip_json_ws(s, pos);
    pos == s.len()
}

fn skip_json_ws(s: &[u16], mut i: usize) -> usize {
    while i < s.len() && matches!(s[i], 0x0020 | 0x0009 | 0x000A | 0x000D) {
        i += 1;
    }
    i
}

#[derive(Clone, Copy)]
enum JsonFrame {
    Array,
    ObjectAfterVal,
}

fn consume_json_value(s: &[u16], pos: &mut usize) -> bool {
    let mut stack: Vec<JsonFrame> = Vec::with_capacity(32);
    const MAX_DEPTH: usize = 1024;

    'expect_value: loop {
        *pos = skip_json_ws(s, *pos);
        let Some(&u) = s.get(*pos) else {
            return false;
        };
        match u {
            0x007B => {
                // `{`
                if stack.len() >= MAX_DEPTH {
                    return false;
                }
                *pos += 1;
                *pos = skip_json_ws(s, *pos);
                if s.get(*pos) == Some(&0x007D) {
                    *pos += 1;
                } else {
                    if !consume_json_string(s, pos) {
                        return false;
                    }
                    *pos = skip_json_ws(s, *pos);
                    if s.get(*pos) != Some(&0x003A) {
                        return false;
                    }
                    *pos += 1;
                    stack.push(JsonFrame::ObjectAfterVal);
                    continue 'expect_value;
                }
            }
            0x005B => {
                // `[`
                if stack.len() >= MAX_DEPTH {
                    return false;
                }
                *pos += 1;
                *pos = skip_json_ws(s, *pos);
                if s.get(*pos) == Some(&0x005D) {
                    *pos += 1;
                } else {
                    stack.push(JsonFrame::Array);
                    continue 'expect_value;
                }
            }
            0x0022 => {
                if !consume_json_string(s, pos) {
                    return false;
                }
            }
            0x0074 => {
                if !starts_with_ascii(&s[*pos..], b"true") {
                    return false;
                }
                *pos += 4;
            }
            0x0066 => {
                if !starts_with_ascii(&s[*pos..], b"false") {
                    return false;
                }
                *pos += 5;
            }
            0x006E => {
                if !starts_with_ascii(&s[*pos..], b"null") {
                    return false;
                }
                *pos += 4;
            }
            0x002D | 0x0030..=0x0039 => {
                if !consume_json_number(s, pos) {
                    return false;
                }
            }
            _ => return false,
        }

        // Value finished; unwind/advance enclosing containers.
        while let Some(frame) = stack.last_mut() {
            *pos = skip_json_ws(s, *pos);
            match frame {
                JsonFrame::Array => match s.get(*pos).copied() {
                    Some(0x002C) => {
                        *pos += 1;
                        continue 'expect_value;
                    }
                    Some(0x005D) => {
                        *pos += 1;
                        stack.pop();
                    }
                    _ => return false,
                },
                JsonFrame::ObjectAfterVal => match s.get(*pos).copied() {
                    Some(0x002C) => {
                        *pos += 1;
                        *pos = skip_json_ws(s, *pos);
                        if !consume_json_string(s, pos) {
                            return false;
                        }
                        *pos = skip_json_ws(s, *pos);
                        if s.get(*pos) != Some(&0x003A) {
                            return false;
                        }
                        *pos += 1;
                        continue 'expect_value;
                    }
                    Some(0x007D) => {
                        *pos += 1;
                        stack.pop();
                    }
                    _ => return false,
                },
            }
        }
        return true;
    }
}

fn consume_json_string(s: &[u16], pos: &mut usize) -> bool {
    if s.get(*pos) != Some(&0x0022) {
        return false;
    }
    *pos += 1;
    while *pos < s.len() {
        let u = s[*pos];
        if u == 0x0022 {
            *pos += 1;
            return true;
        }
        if u < 0x0020 {
            return false;
        }
        if u == 0x005C {
            *pos += 1;
            let Some(&esc) = s.get(*pos) else {
                return false;
            };
            match esc {
                0x0022 | 0x005C | 0x002F | 0x0062 | 0x0066 | 0x006E | 0x0072 | 0x0074 => {
                    *pos += 1;
                }
                0x0075 => {
                    if *pos + 4 >= s.len() {
                        return false;
                    }
                    for &h in &s[*pos + 1..*pos + 5] {
                        if !is_ascii_hex(h) {
                            return false;
                        }
                    }
                    *pos += 5;
                }
                _ => return false,
            }
        } else {
            *pos += 1;
        }
    }
    false
}

fn consume_json_number(s: &[u16], pos: &mut usize) -> bool {
    let mut i = *pos;
    if s.get(i) == Some(&0x002D) {
        i += 1;
    }
    let Some(&first) = s.get(i) else {
        return false;
    };
    if first == 0x0030 {
        i += 1;
    } else if (0x0031..=0x0039).contains(&first) {
        i += 1;
        while i < s.len() && is_ascii_digit(s[i]) {
            i += 1;
        }
    } else {
        return false;
    }
    if s.get(i) == Some(&0x002E) {
        i += 1;
        if i >= s.len() || !is_ascii_digit(s[i]) {
            return false;
        }
        while i < s.len() && is_ascii_digit(s[i]) {
            i += 1;
        }
    }
    if matches!(s.get(i), Some(0x0065 | 0x0045)) {
        i += 1;
        if matches!(s.get(i), Some(0x002B | 0x002D)) {
            i += 1;
        }
        if i >= s.len() || !is_ascii_digit(s[i]) {
            return false;
        }
        while i < s.len() && is_ascii_digit(s[i]) {
            i += 1;
        }
    }
    *pos = i;
    true
}

/// Minifies valid pretty-printed JSON without parsing number values.
/// Returns `None` if `text` is unchanged.
#[must_use]
pub fn minify_json(text: &[u16]) -> Option<Vec<u16>> {
    let s = trim(text);
    if !matches!(s.first().copied(), Some(0x005B | 0x007B)) {
        return None;
    }
    let has_nl_ws = s
        .windows(2)
        .any(|w| w[0] == 0x000A && is_js_whitespace(w[1]));
    if !has_nl_ws || !is_valid_json(s) {
        return None;
    }

    let mut out = Vec::with_capacity(s.len());
    let mut in_str = false;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if in_str {
            out.push(c);
            if c == 0x005C {
                i += 1;
                if let Some(&next) = s.get(i) {
                    out.push(next);
                }
            } else if c == 0x0022 {
                in_str = false;
            }
        } else if c == 0x0022 {
            in_str = true;
            out.push(c);
        } else if !matches!(c, 0x0020 | 0x000A | 0x000D | 0x0009) {
            out.push(c);
        }
        i += 1;
    }
    Some(out)
}

/// Strips `[ \t]+$` at every JS line boundary and collapses 3+ `\n` to `\n\n`.
fn normalize_whitespace_lines(s: &[u16]) -> Vec<u16> {
    let mut trimmed = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if matches!(s[i], 0x0020 | 0x0009) {
            let mut j = i;
            while j < s.len() && matches!(s[j], 0x0020 | 0x0009) {
                j += 1;
            }
            if j == s.len() || is_line_terminator(s[j]) {
                i = j;
                continue;
            }
            trimmed.extend_from_slice(&s[i..j]);
            i = j;
        } else {
            trimmed.push(s[i]);
            i += 1;
        }
    }

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

/// Checks `SALVAGE_RE` at `start` assuming `s` had `/error-like/gi` handled as needed.
fn match_salvage_at(s: &[u16], i: usize) -> Option<usize> {
    if i > 0 && is_ascii_alnum(s[i - 1]) {
        return None;
    }
    let rest = &s[i..];

    if starts_with_ascii_ci(rest, b"err!") {
        return Some(4);
    }

    // Order matches JS alternation for leftmost-longest equivalence.
    if starts_with_ascii_ci(rest, b"timed") || starts_with_ascii_ci(rest, b"time") {
        let t_len = if starts_with_ascii_ci(rest, b"timed") {
            5
        } else {
            4
        };
        let sep_len = if rest
            .get(t_len)
            .is_some_and(|&c| matches!(c, 0x0020 | 0x005F | 0x002D))
        {
            1
        } else {
            0
        };
        if starts_with_ascii_ci(&rest[t_len + sep_len..], b"out") {
            let total = t_len + sep_len + 3;
            if i + total >= s.len() || !is_ascii_alnum(s[i + total]) {
                return Some(total);
            }
        }
    }

    if starts_with_ascii_ci(rest, b"fail") {
        let len = 4usize;
        for suf in [&b"ed"[..], b"ure", b"ing"] {
            if starts_with_ascii_ci(&rest[4..], suf) {
                let candidate = 4 + suf.len();
                if i + candidate >= s.len() || !is_ascii_alnum(s[i + candidate]) {
                    return Some(candidate);
                }
            }
        }
        if i + len >= s.len() || !is_ascii_alnum(s[i + len]) {
            return Some(len);
        }
    }

    if starts_with_ascii_ci(rest, b"assert") {
        if starts_with_ascii_ci(&rest[6..], b"ion")
            && (i + 9 >= s.len() || !is_ascii_alnum(s[i + 9]))
        {
            return Some(9);
        }
        if i + 6 >= s.len() || !is_ascii_alnum(s[i + 6]) {
            return Some(6);
        }
    }

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
    for &kw in STATIC_KEYWORDS {
        if starts_with_ascii_ci(rest, kw)
            && (i + kw.len() >= s.len() || !is_ascii_alnum(s[i + kw.len()]))
        {
            return Some(kw.len());
        }
    }
    None
}

#[must_use]
pub fn find_salvage_match(s: &[u16]) -> Option<(usize, usize)> {
    for i in 0..s.len() {
        if let Some(len) = match_salvage_at(s, i) {
            return Some((i, len));
        }
    }
    None
}

#[must_use]
fn has_raw_salvage(s: &[u16]) -> bool {
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

/// Rescues error-like lines (capping at 300 UTF-16 code units centered on match).
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

#[must_use]
fn is_noise_line(line: &[u16]) -> bool {
    let mut i = 0;
    while i < line.len() && is_js_whitespace(line[i]) {
        i += 1;
    }
    let after_ws = &line[i..];
    let prefix_match = if let Some(&first) = after_ws.first() {
        let icon_or_pass = (matches!(first, 0x2713 | 0x2714 | 0x221A)
            && after_ws.get(1).is_some_and(|&c| is_js_whitespace(c)))
            || (starts_with_ascii(after_ws, b"PASS")
                && after_ws.get(4).is_some_and(|&c| is_js_whitespace(c)))
            || starts_with_ascii(after_ws, b"[PASS]");
        if icon_or_pass {
            true
        } else if starts_with_ascii(after_ws, b"ok ") {
            let mut d = 3;
            while d < after_ws.len() && is_ascii_digit(after_ws[d]) {
                d += 1;
            }
            d > 3 && (d == after_ws.len() || !is_js_word(after_ws[d]))
        } else {
            const WORDS: &[&[u8]] = &[
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
            WORDS.iter().any(|&w| {
                starts_with_ascii(after_ws, w)
                    && (after_ws.len() == w.len() || !is_js_word(after_ws[w.len()]))
            })
        }
    } else {
        false
    };

    if prefix_match {
        return !has_raw_salvage(line);
    }

    // Also `/\sPASSED\b/` anywhere on line (case-sensitive).
    let passed_anywhere = (1..line.len()).any(|k| {
        is_js_whitespace(line[k - 1])
            && starts_with_ascii(&line[k..], b"PASSED")
            && (k + 6 == line.len() || !is_js_word(line[k + 6]))
    });
    passed_anywhere && !has_raw_salvage(line)
}

#[must_use]
pub fn scrub(text: &[u16]) -> Vec<u16> {
    let no_ansi = strip_ansi(text);
    let min = minify_json(&no_ansi);
    let source = min.as_deref().unwrap_or(&no_ansi);
    let normalized = normalize_whitespace_lines(source);

    let lines = split_lf(&normalized);
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
            for _ in 0..n {
                out.push(lines[i].to_vec());
            }
        }
        i = j;
    }

    #[derive(Clone, Copy)]
    enum Entry {
        Line(usize),
        Marker(usize),
    }
    let mut markers: Vec<Vec<u16>> = Vec::new();
    let mut entries: Vec<Entry> = Vec::with_capacity(out.len());
    let mut idx = 0;
    while idx < out.len() {
        let mut j = idx;
        while j < out.len() && is_noise_line(&out[j]) {
            j += 1;
        }
        if j - idx >= NOISE_MIN {
            entries.push(Entry::Line(idx));
            entries.push(Entry::Line(idx + 1));
            let mut note = Vec::new();
            push_ascii(&mut note, "... [puritanical: ");
            push_usize(&mut note, j - idx - 3);
            push_ascii(&mut note, " more passing/progress lines like these] ...");
            entries.push(Entry::Marker(markers.len()));
            markers.push(note);
            entries.push(Entry::Line(j - 1));
        } else {
            let end = j.max(idx + 1);
            for pos in idx..end {
                entries.push(Entry::Line(pos));
            }
            j = end;
        }
        idx = j;
    }

    let slices: Vec<&[u16]> = entries
        .iter()
        .map(|&e| match e {
            Entry::Line(l) => out[l].as_slice(),
            Entry::Marker(m) => markers[m].as_slice(),
        })
        .collect();
    join_u16(&slices, 0x000A)
}

#[must_use]
pub fn duplicate_marker(tool_name: &[u16], text: &[u16], is_earlier: bool) -> Vec<u16> {
    let lines = split_lf(text);
    let mut out = Vec::with_capacity(512);
    push_ascii(&mut out, "[puritanical: output byte-identical to ");
    push_ascii(
        &mut out,
        if is_earlier {
            "an earlier "
        } else {
            "the previous "
        },
    );
    out.extend_from_slice(tool_name);
    push_ascii(&mut out, " result \u{2014} ");
    push_commas(&mut out, text.len());
    push_ascii(&mut out, " chars / ");
    push_usize(&mut out, lines.len());
    push_ascii(&mut out, " lines, unchanged. First lines:]");
    for line in lines.iter().take(5) {
        out.push(0x000A);
        if line.len() > MAX_SALVAGE_LINE {
            out.extend_from_slice(safe_prefix(line, MAX_SALVAGE_LINE - 1));
            out.push(0x2026);
        } else {
            out.extend_from_slice(line);
        }
    }
    out
}

#[must_use]
pub fn compress(
    text: &[u16],
    max_chars: usize,
    head_lines: usize,
    tail_lines: usize,
    spill_path: Option<&[u16]>,
) -> Option<Vec<u16>> {
    let lines = split_lf(text);

    if lines.len() <= head_lines.saturating_add(tail_lines) {
        let keep = max_chars / 2;
        let elided = text.len() as isize - (2 * keep) as isize;
        if elided <= 0 {
            return None;
        }
        let mut out = Vec::with_capacity(max_chars + 160);
        out.extend_from_slice(safe_prefix(text, keep));
        push_ascii(&mut out, "\n... [puritanical: elided ");
        push_commas(&mut out, elided as usize);
        push_ascii(&mut out, " chars from the middle.");
        if let Some(sp) = spill_path
            && !sp.is_empty()
        {
            push_ascii(&mut out, " Full output: ");
            out.extend_from_slice(sp);
            push_ascii(&mut out, " (grep it, do not re-run)");
        }
        push_ascii(&mut out, "] ...\n");
        out.extend_from_slice(safe_suffix(text, keep));
        return Some(out);
    }

    let head = &lines[..head_lines];
    // Matches `lines.slice(-tailLines)` when tail_lines > 0; fallback if 0 matches `lines`.
    let tail_start = if tail_lines == 0 {
        0
    } else {
        lines.len().saturating_sub(tail_lines)
    };
    let tail = &lines[tail_start..];
    let middle = &lines[head_lines..tail_start];

    let mut salvaged: Vec<Vec<u16>> = Vec::new();
    for &line in middle {
        if salvaged.len() >= MAX_SALVAGED {
            break;
        }
        if let Some(cand) = salvage_line(line) {
            salvaged.push(cand);
        }
    }

    let mut marker = Vec::with_capacity(160);
    push_ascii(&mut marker, "... [puritanical: elided ");
    push_commas(&mut marker, middle.len());
    push_ascii(&mut marker, " lines, kept first ");
    push_usize(&mut marker, head_lines);
    push_ascii(&mut marker, ", last ");
    push_usize(&mut marker, tail_lines);
    if !salvaged.is_empty() {
        push_ascii(&mut marker, ", and ");
        push_usize(&mut marker, salvaged.len());
        push_ascii(&mut marker, " error-like line(s) below");
    }
    marker.push(u16::from(b'.'));
    if let Some(sp) = spill_path
        && !sp.is_empty()
    {
        push_ascii(&mut marker, " Full output: ");
        marker.extend_from_slice(sp);
        push_ascii(&mut marker, " (grep it, do not re-run)");
    }
    push_ascii(&mut marker, "] ...");

    let mut parts: Vec<&[u16]> = Vec::with_capacity(head.len() + 1 + salvaged.len() + tail.len());
    parts.extend_from_slice(head);
    parts.push(&marker);
    for s in &salvaged {
        parts.push(s.as_slice());
    }
    parts.extend_from_slice(tail);
    let out = join_u16(&parts, 0x000A);
    (out.len() < text.len()).then_some(out)
}

#[must_use]
pub fn bound_for_opencode(
    text: &[u16],
    spill_path: Option<&[u16]>,
    max_chars: usize,
) -> Option<Vec<u16>> {
    let output_limit = if max_chars == 0 {
        DEFAULT_MAX_CHARS
    } else {
        max_chars
    };
    if text.len() <= output_limit {
        return None;
    }
    if output_limit < 7 {
        return Some(safe_prefix(text, output_limit).to_vec());
    }

    let mut marker = Vec::with_capacity(160);
    push_ascii(
        &mut marker,
        "... [puritanical: rebound compressed output to ",
    );
    push_commas(&mut marker, output_limit);
    push_ascii(&mut marker, " chars.");
    if let Some(sp) = spill_path
        && !sp.is_empty()
    {
        push_ascii(&mut marker, " Full output: ");
        marker.extend_from_slice(sp);
        push_ascii(&mut marker, " (grep it, do not re-run)");
    }
    push_ascii(&mut marker, "] ...");

    if marker.len() > output_limit - 2 {
        let mut truncated = safe_prefix(&marker, output_limit - 7).to_vec();
        push_ascii(&mut truncated, "] ...");
        marker = truncated;
    }

    let mut middle: Vec<Vec<u16>> = vec![marker];
    let mut middle_joined_len = middle[0].len();
    let context_reserve = (output_limit / 4).min(2050);

    for line in split_lf(text) {
        if middle.len() > MAX_SALVAGED {
            break;
        }
        let Some(candidate) = salvage_line(line) else {
            continue;
        };
        if middle_joined_len + candidate.len() + 1 > output_limit.saturating_sub(context_reserve) {
            break;
        }
        if !middle.iter().any(|existing| existing == &candidate) {
            middle_joined_len += 1 + candidate.len();
            middle.push(candidate);
        }
    }

    let middle_refs: Vec<&[u16]> = middle.iter().map(Vec::as_slice).collect();
    let center = join_u16(&middle_refs, 0x000A);
    let remaining = output_limit.saturating_sub(center.len() + 2);
    let head_chars = remaining.div_ceil(2);
    let tail_chars = remaining / 2;

    let head = safe_prefix(text, head_chars);
    let tail = safe_suffix(text, tail_chars);
    let mut out = Vec::with_capacity(head.len() + center.len() + tail.len() + 2);
    out.extend_from_slice(head);
    out.push(0x000A);
    out.extend_from_slice(&center);
    out.push(0x000A);
    out.extend_from_slice(tail);
    Some(out)
}

#[must_use]
fn is_base64ish_slash_plus(u: u16) -> bool {
    is_ascii_alnum(u) || u == u16::from(b'/') || u == u16::from(b'+')
}

#[must_use]
fn is_pem_private_key(text: &[u16]) -> bool {
    let prefix = b"-----BEGIN ";
    let suffix = b"PRIVATE KEY";
    let mut i = 0;
    while i + prefix.len() + suffix.len() <= text.len() {
        if starts_with_ascii(&text[i..], prefix) {
            let mut k = i + prefix.len();
            while k < text.len() && (matches!(text[k], 0x0041..=0x005A | 0x0030..=0x0039 | 0x0020))
            {
                if starts_with_ascii(&text[k..], suffix) {
                    return true;
                }
                k += 1;
            }
        }
        i += 1;
    }
    false
}

#[must_use]
fn has_cloud_or_git_token(text: &[u16]) -> bool {
    let is_token_dash = |u: u16| is_js_word(u) || u == u16::from(b'-');
    let count_run = |slice: &[u16], pred: fn(u16) -> bool| -> usize {
        slice.iter().take_while(|&&u| pred(u)).count()
    };

    for i in 0..text.len() {
        let left_bound = i == 0 || !is_js_word(text[i - 1]);
        if !left_bound {
            continue;
        }
        let rest = &text[i..];

        // (?:AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}\b
        if rest.len() >= 20
            && (starts_with_ascii(rest, b"AKIA")
                || starts_with_ascii(rest, b"ASIA")
                || starts_with_ascii(rest, b"ABIA")
                || starts_with_ascii(rest, b"ACCA"))
            && rest[4..20]
                .iter()
                .all(|&u| is_ascii_upper(u) || is_ascii_digit(u))
            && (i + 20 == text.len() || !is_js_word(text[i + 20]))
        {
            return true;
        }

        // AIza[\w-]{35}
        if starts_with_ascii(rest, b"AIza") && count_run(&rest[4..], is_token_dash) >= 35 {
            return true;
        }
        // ya29\.[\w-]{20,}
        if starts_with_ascii(rest, b"ya29.") && count_run(&rest[5..], is_token_dash) >= 20 {
            return true;
        }
        // GOCSPX-[\w-]{20,}
        if starts_with_ascii(rest, b"GOCSPX-") && count_run(&rest[7..], is_token_dash) >= 20 {
            return true;
        }
        // gh[pousr]_[A-Za-z0-9]{36,}
        if rest.len() >= 40
            && starts_with_ascii(rest, b"gh")
            && matches!(rest[2], 0x0070 | 0x006F | 0x0075 | 0x0073 | 0x0072)
            && rest[3] == u16::from(b'_')
            && count_run(&rest[4..], is_ascii_alnum) >= 36
        {
            return true;
        }
        // github_pat_\w{22,}
        if starts_with_ascii(rest, b"github_pat_") && count_run(&rest[11..], is_js_word) >= 22 {
            return true;
        }
    }
    false
}

#[must_use]
fn has_azure_or_secret_patterns(text: &[u16]) -> bool {
    let is_azure_val = |u: u16| is_ascii_alnum(u) || u == u16::from(b'+') || u == u16::from(b'/');
    let is_sig_val = |u: u16| is_azure_val(u) || u == u16::from(b'%');
    let is_aad_char = |u: u16| is_js_word(u) || matches!(u, 0x007E | 0x002E | 0x002D); // [\w~.-]

    for i in 0..text.len() {
        let rest = &text[i..];

        // aws_secret_access_key["']?[ \t]*[:=][ \t]*["']?[A-Za-z0-9/+]{40}
        let aws_kw = b"aws_secret_access_key";
        if starts_with_ascii_ci(rest, aws_kw) {
            let mut k = aws_kw.len();
            if matches!(rest.get(k), Some(0x0022 | 0x0027)) {
                k += 1;
            }
            while matches!(rest.get(k), Some(0x0020 | 0x0009)) {
                k += 1;
            }
            if matches!(rest.get(k), Some(0x003A | 0x003D)) {
                k += 1;
                while matches!(rest.get(k), Some(0x0020 | 0x0009)) {
                    k += 1;
                }
                if matches!(rest.get(k), Some(0x0022 | 0x0027)) {
                    k += 1;
                }
                if rest
                    .get(k..k + 40)
                    .is_some_and(|s| s.iter().all(|&u| is_base64ish_slash_plus(u)))
                {
                    return true;
                }
            }
        }

        // \b(?:AccountKey|SharedAccessKey)=[A-Za-z0-9+/]{20,}
        if (i == 0 || !is_js_word(text[i - 1]))
            && (starts_with_ascii(rest, b"AccountKey=")
                || starts_with_ascii(rest, b"SharedAccessKey="))
        {
            let k = if starts_with_ascii(rest, b"AccountKey=") {
                11
            } else {
                16
            };
            if rest[k..].iter().take_while(|&&u| is_azure_val(u)).count() >= 20 {
                return true;
            }
        }

        // [?&]sig=[A-Za-z0-9%+/]{20,}
        if (starts_with_ascii(rest, b"?sig=") || starts_with_ascii(rest, b"&sig="))
            && rest[5..].iter().take_while(|&&u| is_sig_val(u)).count() >= 20
        {
            return true;
        }

        // \b[\w~.-]{3}\dQ~[\w~.-]{31,34}
        if rest.len() >= 37
            && has_word_start_only(text, i)
            && is_aad_char(rest[0])
            && is_aad_char(rest[1])
            && is_aad_char(rest[2])
            && is_ascii_digit(rest[3])
            && rest[4] == u16::from(b'Q')
            && rest[5] == u16::from(b'~')
            && rest[6..37].iter().all(|&u| is_aad_char(u))
        {
            return true;
        }
    }
    false
}

/// Exact ECMAScript `\b` match at `i` (character at `i` transitions word-ness vs `i - 1`).
#[must_use]
fn has_word_start_only(s: &[u16], i: usize) -> bool {
    let prev_word = i > 0 && is_js_word(s[i - 1]);
    let curr_word = is_js_word(s[i]);
    prev_word != curr_word
}

#[must_use]
fn is_auth_word(word: &[u16]) -> bool {
    if word.len() >= 24 {
        return true;
    }
    if word.len() < 8 {
        return false;
    }
    word.iter().enumerate().any(|(idx, &u)| {
        is_ascii_digit(u)
            || matches!(
                u,
                0x003D | 0x002B | 0x002F | 0x005F | 0x002E | 0x007E | 0x002D
            )
            || (idx > 0 && is_ascii_upper(u))
    })
}

#[must_use]
fn contains_auth_header(text: &[u16]) -> bool {
    let kw = b"authorization";
    let mut i = 0;
    while i + kw.len() < text.len() {
        if starts_with_ascii_ci(&text[i..], kw) {
            let mut k = i + kw.len();
            if matches!(text.get(k), Some(0x0022 | 0x0027)) {
                k += 1;
            }
            while matches!(text.get(k), Some(0x0020 | 0x0009)) {
                k += 1;
            }
            if matches!(text.get(k), Some(0x003A | 0x003D)) {
                k += 1;
                let cap_end = (k + AUTH_WINDOW).min(text.len());
                let mut end = k;
                while end < cap_end && !matches!(text[end], 0x000A | 0x000D) {
                    end += 1;
                }
                let slice = &text[k..end];
                let has_secret_word = slice
                    .split(|&u| {
                        is_js_whitespace(u) || matches!(u, 0x0022 | 0x0027 | 0x002C | 0x003B)
                    })
                    .any(|w| !w.is_empty() && is_auth_word(w));
                if has_secret_word {
                    return true;
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    false
}

#[must_use]
fn contains_jwt(text: &[u16]) -> bool {
    let is_token_char = |u: u16| is_js_word(u) || u == u16::from(b'.') || u == u16::from(b'-');
    for token in text.split(|&u| !is_token_char(u)) {
        if token.is_empty() {
            continue;
        }
        let parts: Vec<&[u16]> = token.split(|&u| u == u16::from(b'.')).collect();
        if parts.len() < 3 {
            continue;
        }
        for i in 0..parts.len() - 2 {
            if let Some(at) = parts[i]
                .windows(3)
                .position(|w| w == [b'e' as u16, b'y' as u16, b'J' as u16])
                && parts[i].len() - at >= 13
                && parts[i + 1].len() >= 10
                && parts[i + 2].len() >= 10
            {
                return true;
            }
        }
    }
    false
}

#[must_use]
pub fn contains_credential(text: &[u16]) -> bool {
    is_pem_private_key(text)
        || has_cloud_or_git_token(text)
        || has_azure_or_secret_patterns(text)
        || contains_jwt(text)
        || contains_auth_header(text)
}

#[cfg(test)]
mod tests {
    use super::{
        bound_for_opencode, compress, contains_credential, duplicate_marker, minify_json,
        salvage_line, scrub, unsafe_tool_name,
    };
    use crate::utf16::{str_from_u16_lossy, u16_from_str};

    #[test]
    fn basic_compress_and_scrub() {
        assert!(unsafe_tool_name(&u16_from_str("mcp__fs__read_file")));
        assert!(!unsafe_tool_name(&u16_from_str("Bash")));
        assert!(!contains_credential(&u16_from_str(
            "Authorization: token required"
        )));
        assert!(contains_credential(&u16_from_str(
            "Authorization: Bearer abcdefgh1234"
        )));
        assert_eq!(
            str_from_u16_lossy(&minify_json(&u16_from_str("{\n  \"a\": 1\n}")).unwrap()),
            r#"{"a":1}"#
        );
        assert!(minify_json(&u16_from_str("{\n  \"a\": 01\n}")).is_none());
        let dup = str_from_u16_lossy(&duplicate_marker(
            &u16_from_str("Bash"),
            &u16_from_str("hi\nthere"),
            false,
        ));
        assert!(dup.contains("the previous Bash result"));
        assert!(salvage_line(&u16_from_str("this error-like placeholder")).is_none());
        let big = u16_from_str(
            &(0..60)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let comp = compress(&big, 100, 3, 3, None).unwrap();
        assert!(comp.len() < big.len());
        let sc = scrub(&u16_from_str("a  \n\n\n\nb"));
        assert_eq!(str_from_u16_lossy(&sc), "a\n\nb");
        let bnd = bound_for_opencode(&big, None, 120).unwrap();
        assert!(bnd.len() <= 120);
    }
}
