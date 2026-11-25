#![forbid(unsafe_code)]

//! ANSI escape stripping and zero-copy-style JSON minification for compressed output.

use super::{is_ascii_digit, is_ascii_hex};
use crate::utf16::{is_js_whitespace, starts_with_ascii, trim};

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

fn match_csi(s: &[u16], i: usize) -> Option<usize> {
    let mut k = i + 2;
    while k < s.len() && matches!(s[k], 0x0030..=0x0039 | 0x003B | 0x003F) {
        k += 1;
    }
    while k < s.len() && (0x0020..=0x002F).contains(&s[k]) {
        k += 1;
    }
    (k < s.len() && (0x0040..=0x007E).contains(&s[k])).then(|| k + 1 - i)
}

fn match_osc(s: &[u16], i: usize) -> Option<usize> {
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
    None
}

fn match_ansi_at(s: &[u16], i: usize) -> Option<usize> {
    match *s.get(i + 1)? {
        0x005B => match_csi(s, i),
        0x005D => match_osc(s, i),
        _ => None,
    }
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

#[derive(Clone, Copy)]
enum JsonStep {
    NeedValue,
    ValueDone,
    Invalid,
}

const MAX_JSON_DEPTH: usize = 1024;

fn open_json_object(s: &[u16], pos: &mut usize, stack: &mut Vec<JsonFrame>) -> JsonStep {
    if stack.len() >= MAX_JSON_DEPTH {
        return JsonStep::Invalid;
    }
    *pos = skip_json_ws(s, *pos + 1);
    if s.get(*pos) == Some(&0x007D) {
        *pos += 1;
        return JsonStep::ValueDone;
    }
    if !consume_json_string(s, pos) {
        return JsonStep::Invalid;
    }
    *pos = skip_json_ws(s, *pos);
    if s.get(*pos) != Some(&0x003A) {
        return JsonStep::Invalid;
    }
    *pos += 1;
    stack.push(JsonFrame::ObjectAfterVal);
    JsonStep::NeedValue
}

fn open_json_array(s: &[u16], pos: &mut usize, stack: &mut Vec<JsonFrame>) -> JsonStep {
    if stack.len() >= MAX_JSON_DEPTH {
        return JsonStep::Invalid;
    }
    *pos = skip_json_ws(s, *pos + 1);
    if s.get(*pos) == Some(&0x005D) {
        *pos += 1;
        return JsonStep::ValueDone;
    }
    stack.push(JsonFrame::Array);
    JsonStep::NeedValue
}

fn consume_literal(s: &[u16], pos: &mut usize, lit: &[u8]) -> JsonStep {
    if !starts_with_ascii(&s[*pos..], lit) {
        return JsonStep::Invalid;
    }
    *pos += lit.len();
    JsonStep::ValueDone
}

fn step_json_value(s: &[u16], pos: &mut usize, stack: &mut Vec<JsonFrame>) -> JsonStep {
    *pos = skip_json_ws(s, *pos);
    let Some(&u) = s.get(*pos) else {
        return JsonStep::Invalid;
    };
    match u {
        0x007B => open_json_object(s, pos, stack),
        0x005B => open_json_array(s, pos, stack),
        0x0022 if consume_json_string(s, pos) => JsonStep::ValueDone,
        0x0074 => consume_literal(s, pos, b"true"),
        0x0066 => consume_literal(s, pos, b"false"),
        0x006E => consume_literal(s, pos, b"null"),
        0x002D | 0x0030..=0x0039 if consume_json_number(s, pos) => JsonStep::ValueDone,
        _ => JsonStep::Invalid,
    }
}

fn advance_json_container(s: &[u16], pos: &mut usize, stack: &mut Vec<JsonFrame>) -> JsonStep {
    while let Some(&frame) = stack.last() {
        *pos = skip_json_ws(s, *pos);
        let next = s.get(*pos).copied();
        match (frame, next) {
            (JsonFrame::Array, Some(0x002C)) => {
                *pos += 1;
                return JsonStep::NeedValue;
            }
            (JsonFrame::Array, Some(0x005D)) | (JsonFrame::ObjectAfterVal, Some(0x007D)) => {
                *pos += 1;
                stack.pop();
            }
            (JsonFrame::ObjectAfterVal, Some(0x002C)) => {
                *pos = skip_json_ws(s, *pos + 1);
                if !consume_json_string(s, pos) {
                    return JsonStep::Invalid;
                }
                *pos = skip_json_ws(s, *pos);
                if s.get(*pos) != Some(&0x003A) {
                    return JsonStep::Invalid;
                }
                *pos += 1;
                return JsonStep::NeedValue;
            }
            _ => return JsonStep::Invalid,
        }
    }
    JsonStep::ValueDone
}

fn consume_json_value(s: &[u16], pos: &mut usize) -> bool {
    let mut stack: Vec<JsonFrame> = Vec::with_capacity(32);
    loop {
        match step_json_value(s, pos, &mut stack) {
            JsonStep::Invalid => return false,
            JsonStep::NeedValue => continue,
            JsonStep::ValueDone => {}
        }
        match advance_json_container(s, pos, &mut stack) {
            JsonStep::Invalid => return false,
            JsonStep::NeedValue => continue,
            JsonStep::ValueDone => return true,
        }
    }
}

fn is_valid_json(s: &[u16]) -> bool {
    let mut pos = skip_json_ws(s, 0);
    if !consume_json_value(s, &mut pos) {
        return false;
    }
    skip_json_ws(s, pos) == s.len()
}

fn consume_json_escape(s: &[u16], pos: &mut usize) -> bool {
    *pos += 1;
    let Some(&esc) = s.get(*pos) else {
        return false;
    };
    if matches!(
        esc,
        0x0022 | 0x005C | 0x002F | 0x0062 | 0x0066 | 0x006E | 0x0072 | 0x0074
    ) {
        *pos += 1;
        return true;
    }
    if esc == 0x0075 && *pos + 4 < s.len() && s[*pos + 1..*pos + 5].iter().all(|&h| is_ascii_hex(h))
    {
        *pos += 5;
        return true;
    }
    false
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
            if !consume_json_escape(s, pos) {
                return false;
            }
        } else {
            *pos += 1;
        }
    }
    false
}

fn scan_digits(s: &[u16], mut i: usize) -> usize {
    while i < s.len() && is_ascii_digit(s[i]) {
        i += 1;
    }
    i
}

fn consume_json_number(s: &[u16], pos: &mut usize) -> bool {
    let mut i = *pos + usize::from(s.get(*pos) == Some(&0x002D));
    match s.get(i).copied() {
        Some(0x0030) => i += 1,
        Some(0x0031..=0x0039) => i = scan_digits(s, i + 1),
        _ => return false,
    }
    if s.get(i) == Some(&0x002E) {
        let end = scan_digits(s, i + 1);
        if end == i + 1 {
            return false;
        }
        i = end;
    }
    if matches!(s.get(i), Some(0x0065 | 0x0045)) {
        i += 1 + usize::from(matches!(s.get(i + 1), Some(0x002B | 0x002D)));
        let end = scan_digits(s, i);
        if end == i {
            return false;
        }
        i = end;
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
