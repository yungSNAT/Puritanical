#![forbid(unsafe_code)]

//! Line-drop filters for git status hints, install/build/git progress and tests.

use super::classify::{KIND_GIT_STATUS, KIND_PROGRESS, KIND_TEST};
use super::tests_filter::matches_test_drop;
use super::{is_ascii_digit, is_ascii_hex_lower};
use crate::utf16::{
    eq_ascii, find_ascii_ci, is_js_whitespace, starts_with_ascii, trim, trim_start,
};

fn is_pipeline_safe(line: &[u16]) -> bool {
    const TERMS: &[&[u8]] = &[
        b"error",
        b"fail",
        b"warn",
        b"denied",
        b"fatal",
        b"panic",
        b"exception",
        b"traceback",
    ];
    TERMS.iter().any(|t| find_ascii_ci(line, t).is_some())
}

pub(crate) fn should_drop_line(kinds: u16, line: &[u16]) -> bool {
    let safe_allow = is_pipeline_safe(line);
    if kinds & KIND_GIT_STATUS != 0 && matches_git_status_hint(line) && !safe_allow {
        return true;
    }
    if kinds & KIND_PROGRESS != 0 && !safe_allow && matches_progress_drop(line) {
        return true;
    }
    kinds & KIND_TEST != 0 && matches_test_drop(line)
}

fn matches_git_status_hint(l: &[u16]) -> bool {
    let s = trim_start(l);
    if s.first() != Some(&u16::from(b'(')) {
        return false;
    }
    let after_open = &s[1..];
    if !(starts_with_ascii(after_open, b"use \"git ")
        || starts_with_ascii(after_open, b"commit or discard "))
    {
        return false;
    }
    after_open
        .iter()
        .position(|&u| u == u16::from(b')'))
        .is_some_and(|close| trim(&after_open[close + 1..]).is_empty())
}

fn matches_git_progress(l: &[u16]) -> bool {
    let no_remote = l.strip_prefix(&[
        b'r' as u16,
        b'e' as u16,
        b'm' as u16,
        b'o' as u16,
        b't' as u16,
        b'e' as u16,
        b':' as u16,
        b' ' as u16,
    ]);
    let after = no_remote.unwrap_or(l);
    const PREFIXES: &[&[u8]] = &[
        b"Enumerating objects:",
        b"Counting objects:",
        b"Compressing objects:",
        b"Writing objects:",
        b"Receiving objects:",
        b"Unpacking objects:",
        b"Resolving deltas:",
    ];
    if PREFIXES.iter().any(|&p| starts_with_ascii(after, p)) {
        return true;
    }
    if starts_with_ascii(after, b"Total ") {
        let mut k = 6;
        while k < after.len() && is_ascii_digit(after[k]) {
            k += 1;
        }
        if k > 6 && starts_with_ascii(&after[k..], b" (delta") {
            return true;
        }
    }
    no_remote.is_none()
        && (starts_with_ascii(l, b"Delta compression using")
            || starts_with_ascii(l, b"Updating files:")
            || starts_with_ascii(l, b"Checking out files:"))
}

fn matches_npm_progress(l: &[u16]) -> bool {
    let k = l.iter().take_while(|&&u| is_ascii_digit(u)).count();
    if k > 0
        && [
            &b" package is looking for funding"[..],
            b" packages are looking for funding",
            b" package are looking for funding",
            b" packages is looking for funding",
        ]
        .iter()
        .any(|&suf| eq_ascii(&l[k..], suf))
    {
        return true;
    }
    if eq_ascii(trim_start(l), b"run `npm fund` for details") {
        return true;
    }
    if starts_with_ascii(l, b"Progress: resolved ") && l.get(19).is_some_and(|&u| is_ascii_digit(u))
    {
        return true;
    }
    l.len() >= 5 && l.iter().all(|&u| u == u16::from(b'+'))
}

fn matches_pip_progress(l: &[u16]) -> bool {
    if starts_with_ascii(l, b"Requirement already satisfied: ") {
        return true;
    }
    let ws = trim_start(l);
    // Original JS `/^\s*━+/` matches any line whose non-whitespace content starts with U+2501.
    if ws.first() == Some(&0x2501) {
        return true;
    }
    [
        &b"Collecting "[..],
        b"Downloading ",
        b"Using cached ",
        b"Obtaining ",
    ]
    .iter()
    .any(|&w| starts_with_ascii(ws, w) && ws.get(w.len()).is_some_and(|&u| !is_js_whitespace(u)))
}

fn matches_cargo_go_progress(l: &[u16]) -> bool {
    if !l.is_empty() && is_js_whitespace(l[0]) {
        let ws = trim_start(l);
        if eq_ascii(ws, b"Updating crates.io index") {
            return true;
        }
        let hit = [
            &b"Compiling "[..],
            b"Checking ",
            b"Downloaded ",
            b"Downloading ",
            b"Fresh ",
            b"Locking ",
            b"Adding ",
            b"Blocking ",
        ]
        .iter()
        .any(|&w| {
            starts_with_ascii(ws, w) && ws.get(w.len()).is_some_and(|&u| !is_js_whitespace(u))
        });
        if hit {
            return true;
        }
    }
    starts_with_ascii(l, b"go: downloading ")
        || starts_with_ascii(l, b"go: finding ")
        || starts_with_ascii(l, b"go: extracting ")
}

fn is_buildkit_sha_transfer(r: &[u16]) -> bool {
    if !starts_with_ascii(r, b"sha256:") {
        return false;
    }
    let mut h = 7;
    while h < r.len() && is_ascii_hex_lower(r[h]) {
        h += 1;
    }
    if h == 7 || r.get(h) != Some(&u16::from(b' ')) {
        return false;
    }
    let mut n = h + 1;
    while n < r.len() && (is_ascii_digit(r[n]) || r[n] == u16::from(b'.')) {
        n += 1;
    }
    if n == h + 1 {
        return false;
    }
    n += usize::from(matches!(r.get(n), Some(0x006B | 0x004D | 0x0047)));
    starts_with_ascii(&r[n..], b"B /")
}

fn matches_docker_progress(l: &[u16]) -> bool {
    if l.len() >= 14
        && l[..12].iter().all(|&u| is_ascii_hex_lower(u))
        && starts_with_ascii(&l[12..], b": ")
    {
        let msgs: &[&[u8]] = &[
            b"Pulling fs layer",
            b"Waiting",
            b"Verifying Checksum",
            b"Download complete",
            b"Pull complete",
            b"Already exists",
            b"Downloading",
            b"Extracting",
        ];
        if msgs.iter().any(|&m| starts_with_ascii(&l[14..], m)) {
            return true;
        }
    }
    if l.first() == Some(&u16::from(b'#')) {
        let mut k = 1;
        while k < l.len() && is_ascii_digit(l[k]) {
            k += 1;
        }
        if k > 1 && l.get(k) == Some(&u16::from(b' ')) {
            let r = &l[k + 1..];
            return starts_with_ascii(r, b"transferring ")
                || starts_with_ascii(r, b"extracting sha256:")
                || is_buildkit_sha_transfer(r);
        }
    }
    false
}

fn matches_progress_drop(l: &[u16]) -> bool {
    matches_git_progress(l)
        || matches_npm_progress(l)
        || matches_pip_progress(l)
        || matches_cargo_go_progress(l)
        || matches_docker_progress(l)
}
