#![forbid(unsafe_code)]

//! Port of `hooks/puritanical-mode.js` (`requestedMode`).

use crate::utf16::{
    ascii_lower, eq_ascii, has_word_bounds, is_js_whitespace, is_js_word, is_line_terminator,
    starts_with_ascii, trim,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    On,
}

const OFF_ARGS: &[&[u8]] = &[b"off", b"stop", b"disable", b"deactivate"];

fn skip_ws(s: &[u16], mut i: usize) -> usize {
    while i < s.len() && is_js_whitespace(s[i]) {
        i += 1;
    }
    i
}

fn contains_word_seq(line: &[u16], first: &[&[u8]], second: &[u8]) -> bool {
    let mut i = 0;
    while i < line.len() {
        if i == 0 || !is_js_word(line[i - 1]) {
            for &kw in first {
                if starts_with_ascii(&line[i..], kw) {
                    let after_kw = i + kw.len();
                    let j = skip_ws(line, after_kw);
                    if j > after_kw && starts_with_ascii(&line[j..], second) {
                        let end = j + second.len();
                        if end == line.len() || !is_js_word(line[end]) {
                            return true;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn contains_chisle_off_after(s: &[u16]) -> bool {
    let mut i = 0;
    while i < s.len() {
        if (i == 0 || !is_js_word(s[i - 1])) && starts_with_ascii(&s[i..], b"chisle") {
            let after_chisle = i + b"chisle".len();
            let mut j = skip_ws(s, after_chisle);
            if j > after_chisle {
                if starts_with_ascii(&s[j..], b"mode") {
                    let after_mode = j + b"mode".len();
                    let k = skip_ws(s, after_mode);
                    if k > after_mode {
                        j = k;
                    }
                }
                for &arg in OFF_ARGS {
                    if starts_with_ascii(&s[j..], arg) {
                        let end = j + arg.len();
                        if end == s.len() || !is_js_word(s[end]) {
                            return true;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn has_word(line: &[u16], words: &[&[u8]]) -> Option<usize> {
    let mut i = 0;
    while i < line.len() {
        if i == 0 || !is_js_word(line[i - 1]) {
            for &w in words {
                if starts_with_ascii(&line[i..], w) {
                    let end = i + w.len();
                    if end == line.len() || !is_js_word(line[end]) {
                        return Some(end);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

fn contains_word_pair_on_line(line: &[u16], first: &[&[u8]], second: &[&[u8]]) -> bool {
    if let Some(after_first) = has_word(line, first) {
        return has_word(&line[after_first..], second).is_some();
    }
    false
}

fn contains_chislify(s: &[u16]) -> bool {
    let mut i = 0;
    while i < s.len() {
        if (i == 0 || !is_js_word(s[i - 1])) && starts_with_ascii(&s[i..], b"chislif") {
            let rest = &s[i + b"chislif".len()..];
            let suffix_len = if starts_with_ascii(rest, b"ier") {
                3
            } else if starts_with_ascii(rest, b"y") {
                1
            } else {
                0
            };
            if suffix_len > 0 && has_word_bounds(s, i, i + b"chislif".len() + suffix_len) {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn legacy_mode(text: &[u16]) -> Option<Mode> {
    let chisle_off_verbs: &[&[u8]] = &[
        b"turn off",
        b"disable",
        b"deactivate",
        b"stop",
        b"kill",
        b"exit",
    ];
    if contains_word_seq(text, chisle_off_verbs, b"chisle") || contains_chisle_off_after(text) {
        return Some(Mode::Off);
    }
    if starts_with_ascii(text, b"/chisle") {
        let mut after = b"/chisle".len();
        if starts_with_ascii(&text[after..], b":chisle") {
            after += b":chisle".len();
        }
        if after == text.len() || !is_js_word(text[after]) {
            return Some(Mode::On);
        }
    }
    let on_verbs: &[&[u8]] = &[b"activate", b"enable", b"turn on", b"start", b"use"];
    let on_nouns: &[&[u8]] = &[b"mode", b"activate", b"enable", b"on"];
    let on_line_match = text.split(|&u| is_line_terminator(u)).any(|line| {
        contains_word_pair_on_line(line, on_verbs, &[b"chisle"])
            || contains_word_pair_on_line(line, &[b"chisle"], on_nouns)
    });
    if on_line_match || contains_chislify(text) {
        return Some(Mode::On);
    }
    None
}

fn match_puritanical_order(order: &[u16]) -> Option<Mode> {
    if starts_with_ascii(order, b"puritanical") {
        let mut i = b"puritanical".len();
        let after_ws = skip_ws(order, i);
        if after_ws > i {
            i = after_ws;
            if starts_with_ascii(&order[i..], b"mode") {
                let after_mode = i + b"mode".len();
                if after_mode == order.len() {
                    return Some(Mode::On);
                }
                let next = skip_ws(order, after_mode);
                if next > after_mode {
                    if eq_ascii(&order[next..], b"on") {
                        return Some(Mode::On);
                    }
                    if eq_ascii(&order[next..], b"off") {
                        return Some(Mode::Off);
                    }
                }
            } else if eq_ascii(&order[i..], b"on") {
                return Some(Mode::On);
            } else if eq_ascii(&order[i..], b"off") {
                return Some(Mode::Off);
            }
        }
    }

    // Strip trailing `\s+mode$` cleanly without mutating.
    let without_mode = strip_trailing_mode(order);
    let on_prefixes: &[&[u8]] = &[b"activate", b"enable", b"turn on", b"start"];
    let off_prefixes: &[&[u8]] = &[
        b"stop",
        b"disable",
        b"deactivate",
        b"turn off",
        b"exit",
        b"kill",
    ];
    if matches_verb_space_puritanical(without_mode, on_prefixes) {
        return Some(Mode::On);
    }
    if matches_verb_space_puritanical(without_mode, off_prefixes) {
        return Some(Mode::Off);
    }
    None
}

fn strip_trailing_mode(s: &[u16]) -> &[u16] {
    if s.len() > b"mode".len() && starts_with_ascii(&s[s.len() - b"mode".len()..], b"mode") {
        let mut ws_start = s.len() - b"mode".len();
        let orig = ws_start;
        while ws_start > 0 && is_js_whitespace(s[ws_start - 1]) {
            ws_start -= 1;
        }
        if ws_start < orig {
            return &s[..ws_start];
        }
    }
    s
}

fn matches_verb_space_puritanical(s: &[u16], verbs: &[&[u8]]) -> bool {
    for &verb in verbs {
        if starts_with_ascii(s, verb) {
            let after_ws = skip_ws(s, verb.len());
            if after_ws > verb.len() && eq_ascii(&s[after_ws..], b"puritanical") {
                return true;
            }
        }
    }
    false
}

/// Parses a prompt for explicit `/puritanical` or legacy Chisle mode toggles.
#[must_use]
pub fn requested_mode(prompt: &[u16]) -> Option<Mode> {
    let trimmed = trim(prompt);
    if trimmed.is_empty() {
        return None;
    }
    let text = ascii_lower(trimmed);

    if contains_word_seq(&text, &[b"normal"], b"mode") {
        return Some(Mode::Off);
    }
    if let Some(mode) = legacy_mode(&text) {
        return Some(mode);
    }

    if starts_with_ascii(&text, b"/puritanical") {
        let mut after = b"/puritanical".len();
        if starts_with_ascii(&text[after..], b":puritanical") {
            after += b":puritanical".len();
        }
        if after == text.len() || !is_js_word(text[after]) {
            let rest = &text[after..];
            let arg_start = skip_ws(rest, 0);
            if arg_start > 0 && arg_start < rest.len() {
                let mut arg_end = arg_start;
                while arg_end < rest.len() && !is_js_whitespace(rest[arg_end]) {
                    arg_end += 1;
                }
                let arg = &rest[arg_start..arg_end];
                if OFF_ARGS.iter().any(|&a| eq_ascii(arg, a)) {
                    return Some(Mode::Off);
                }
            }
            return Some(Mode::On);
        }
    }

    let mut order: &[u16] = &text;
    if starts_with_ascii(order, b"please") {
        let ws = skip_ws(order, b"please".len());
        if ws > b"please".len() {
            order = &order[ws..];
        }
    }
    while matches!(order.last().copied(), Some(0x002E | 0x0021)) {
        order = &order[..order.len() - 1];
    }

    match_puritanical_order(order)
}

#[cfg(test)]
mod tests {
    use super::{Mode, requested_mode};
    use crate::utf16::u16_from_str;

    fn eval(s: &str) -> Option<Mode> {
        requested_mode(&u16_from_str(s))
    }

    #[test]
    fn basic_directives() {
        assert_eq!(eval("normal mode"), Some(Mode::Off));
        assert_eq!(eval("/puritanical"), Some(Mode::On));
        assert_eq!(eval("/puritanical off"), Some(Mode::Off));
        assert_eq!(eval("please puritanical mode on!"), Some(Mode::On));
        assert_eq!(eval("puritanical is an everyday word"), None);
        assert_eq!(eval("enable\nchisle"), None);
        assert_eq!(eval("enable chisle"), Some(Mode::On));
    }
}
