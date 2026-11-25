#![forbid(unsafe_code)]

//! Port of `hooks/puritanical-command-filters.js`.

use crate::compress::strip_ansi;
use crate::utf16::{
    contains_ascii_word, eq_ascii, eq_ascii_ci, find_ascii, find_ascii_ci, find_subslice,
    is_js_whitespace, is_js_word, is_line_terminator, join_u16, push_ascii, push_usize, split_lf,
    starts_with_ascii, trim, trim_end, trim_start,
};

pub const KIND_GIT_STATUS: u16 = 1 << 0;
pub const KIND_PROGRESS: u16 = 1 << 1;
pub const KIND_GIT_LOG: u16 = 1 << 2;
pub const KIND_GIT_DIFF: u16 = 1 << 3;
pub const KIND_GIT_BRANCH: u16 = 1 << 4;
pub const KIND_LS: u16 = 1 << 5;
pub const KIND_WC: u16 = 1 << 6;
pub const KIND_GREP: u16 = 1 << 7;
pub const KIND_TEST: u16 = 1 << 8;

/// All kinds in stable `KIND_RULES` order for JS `Set` iteration parity.
pub const KIND_ORDER: &[(u16, &str)] = &[
    (KIND_GIT_STATUS, "git-status"),
    (KIND_PROGRESS, "progress"),
    (KIND_GIT_LOG, "git-log"),
    (KIND_GIT_DIFF, "git-diff"),
    (KIND_GIT_BRANCH, "git-branch"),
    (KIND_LS, "ls"),
    (KIND_WC, "wc"),
    (KIND_GREP, "grep"),
    (KIND_TEST, "test"),
];

const RUNNERS: &[&[u8]] = &[
    b"sudo", b"time", b"command", b"exec", b"nice", b"env", b"npx", b"bunx", b"pnpx",
];
const TEST_PROGRAMS: &[&[u8]] = &[
    b"pytest", b"py.test", b"jest", b"vitest", b"mocha", b"ava", b"tap", b"rspec", b"phpunit",
];
const PKG_MANAGERS: &[&[u8]] = &[b"npm", b"pnpm", b"yarn", b"bun"];

#[must_use]
fn is_ascii_digit(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039)
}

#[must_use]
fn is_ascii_alpha(u: u16) -> bool {
    matches!(u, 0x0041..=0x005A | 0x0061..=0x007A)
}

#[must_use]
fn is_ascii_hex_lower(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0061..=0x0066)
}

#[must_use]
pub fn env_assign_end(s: &[u16], start: usize) -> usize {
    let mut end = start.min(s.len());
    loop {
        // ENV_NAME = /[A-Za-z_]\w*=/y
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
        i += 1;

        // ENV_VALUE* = /'[^']*'|"(?:\\.|[^"\\])*"|[^\s'"\\]+/y
        while i < s.len() {
            if let Some(next) = match_single_quote(s, i) {
                i = next;
            } else if let Some(next) = match_double_quote(s, i) {
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

        // ENV_SPACE = /\s+/y
        let before_ws = i;
        while i < s.len() && is_js_whitespace(s[i]) {
            i += 1;
        }
        if i == before_ws {
            return end;
        }
        end = i;
    }
}

fn match_single_quote(s: &[u16], start: usize) -> Option<usize> {
    if s.get(start) != Some(&0x0027) {
        return None;
    }
    let mut i = start + 1;
    while i < s.len() {
        if s[i] == 0x0027 {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn match_double_quote(s: &[u16], start: usize) -> Option<usize> {
    if s.get(start) != Some(&0x0022) {
        return None;
    }
    let mut i = start + 1;
    while i < s.len() {
        let u = s[i];
        if u == 0x0022 {
            return Some(i + 1);
        }
        if u == 0x005C {
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
        if let Some(end) = match_single_quote(cmd, i) {
            out.push(0x0022);
            out.push(0x0022);
            i = end;
            continue;
        }
        if let Some(end) = match_double_quote(cmd, i) {
            out.push(0x0022);
            out.push(0x0022);
            i = end;
            continue;
        }
        out.push(cmd[i]);
        i += 1;
    }
    out
}

fn split_ws(s: &[u16]) -> Vec<&[u16]> {
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

fn runner_pair_end(s: &[u16]) -> Option<usize> {
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
    for &p in EXACT_PAIRS {
        if starts_with_ascii(s, p) {
            let mut k = p.len();
            if k < s.len() && is_js_whitespace(s[k]) {
                while k < s.len() && is_js_whitespace(s[k]) {
                    k += 1;
                }
                return Some(k);
            }
        }
    }
    if starts_with_ascii(s, b"python") {
        let mut k = 6;
        while k < s.len() && (is_ascii_digit(s[k]) || s[k] == u16::from(b'.')) {
            k += 1;
        }
        if starts_with_ascii(&s[k..], b" -m") {
            let mut j = k + 3;
            if j < s.len() && is_js_whitespace(s[j]) {
                while j < s.len() && is_js_whitespace(s[j]) {
                    j += 1;
                }
                return Some(j);
            }
        }
    }
    None
}

fn strip_runners(segment: &[u16]) -> Vec<u16> {
    let end = env_assign_end(segment, 0);
    let mut s = segment[end..].to_vec();
    loop {
        let first_word = s
            .iter()
            .copied()
            .take_while(|&u| !is_js_whitespace(u))
            .collect::<Vec<u16>>();
        if RUNNERS.iter().any(|&r| eq_ascii(&first_word, r)) {
            let trimmed = trim(&s[first_word.len()..]).to_vec();
            let env_end = env_assign_end(&trimmed, 0);
            s = trimmed[env_end..].to_vec();
            continue;
        }
        if let Some(pair_end) = runner_pair_end(&s) {
            s = s[pair_end..].to_vec();
            continue;
        }
        return s;
    }
}

struct Classified {
    kinds: u16,
    prog: Vec<u16>,
    sub: Vec<u16>,
    flags: Vec<Vec<u16>>,
    text: Vec<u16>,
}

impl Classified {
    fn has_flag(&self, flag: &[u8]) -> bool {
        self.flags.iter().any(|f| eq_ascii(f, flag))
    }

    fn prog_eq(&self, name: &[u8]) -> bool {
        eq_ascii(&self.prog, name)
    }

    fn sub_in(&self, names: &[&[u8]]) -> bool {
        names.iter().any(|&n| eq_ascii(&self.sub, n))
    }
}

fn classify(segment: &[u16]) -> Option<Classified> {
    let parts: Vec<&[u16]> = segment.split(|&u| u == u16::from(b'|')).map(trim).collect();
    for p in parts.iter().skip(1) {
        let first = p.split(|&u| is_js_whitespace(u)).next().unwrap_or(&[]);
        if !eq_ascii(first, b"head") && !eq_ascii(first, b"tail") {
            return None;
        }
    }

    let s = strip_runners(parts[0]);
    let words: Vec<&[u16]> = split_ws(&s)
        .into_iter()
        .filter(|w| {
            !w.is_empty()
                && !matches!(w[0], 0x003C | 0x003E | 0x0026)
                && !(is_ascii_digit(w[0]) && w.get(1) == Some(&0x003E))
        })
        .collect();

    let raw_prog = words.first().copied().unwrap_or(&[]);
    let after_sep = match raw_prog
        .iter()
        .rposition(|&u| u == u16::from(b'/') || u == u16::from(b'\\'))
    {
        Some(idx) => &raw_prog[idx + 1..],
        None => raw_prog,
    };
    let prog = if after_sep.len() >= 4
        && (eq_ascii_ci(&after_sep[after_sep.len() - 4..], b".exe")
            || eq_ascii_ci(&after_sep[after_sep.len() - 4..], b".cmd"))
    {
        after_sep[..after_sep.len() - 4].to_vec()
    } else {
        after_sep.to_vec()
    };

    if eq_ascii(&prog, b"rtk") {
        return None;
    }

    let mut sub = Vec::new();
    let mut i = 1;
    while i < words.len() {
        let w = words[i];
        if w.len() == 2 && w[0] == u16::from(b'-') && matches!(w[1], 0x0043 | 0x0063) {
            i += 2;
            continue;
        }
        if !w.starts_with(&[u16::from(b'-')]) {
            sub = w.to_vec();
            break;
        }
        i += 1;
    }

    let mut flags: Vec<Vec<u16>> = Vec::new();
    for &w in words.iter().skip(1) {
        if w.len() >= 3 && w[0] == u16::from(b'-') && w[1] == u16::from(b'-') && is_js_word(w[2]) {
            let eq_end = w
                .iter()
                .position(|&u| u == u16::from(b'='))
                .unwrap_or(w.len());
            flags.push(w[..eq_end].to_vec());
        } else if w.len() >= 2
            && w[0] == u16::from(b'-')
            && w[1..].iter().all(|&u| is_ascii_alpha(u))
        {
            for &ch in &w[1..] {
                flags.push(vec![ch]);
            }
        }
    }

    let mut c = Classified {
        kinds: 0,
        prog,
        sub,
        flags,
        text: s,
    };
    c.kinds = compute_kinds(&c);
    (c.kinds != 0).then_some(c)
}

fn contains_ws_flag(text: &[u16], flag: &[u8]) -> bool {
    (1..text.len()).any(|i| {
        is_js_whitespace(text[i - 1])
            && starts_with_ascii(&text[i..], flag)
            && (i + flag.len() == text.len() || !is_js_word(text[i + flag.len()]))
    })
}

fn has_git_log_skip_flag(text: &[u16]) -> bool {
    for pat in [&b"--format"[..], b"--pretty", b"--oneline", b"--graph"] {
        for i in 0..text.len() {
            if starts_with_ascii(&text[i..], pat)
                && (i + pat.len() == text.len() || !is_js_word(text[i + pat.len()]))
            {
                return true;
            }
        }
    }
    false
}

fn is_pip_prog(prog: &[u16]) -> bool {
    if !starts_with_ascii(prog, b"pip") {
        return false;
    }
    let rest = &prog[3..];
    if rest.is_empty() || rest.iter().all(|&u| is_ascii_digit(u)) {
        return true;
    }
    if let Some(dot) = rest.iter().position(|&u| u == u16::from(b'.')) {
        let before = &rest[..dot];
        let after = &rest[dot + 1..];
        return before.iter().all(|&u| is_ascii_digit(u))
            && !after.is_empty()
            && after.iter().all(|&u| is_ascii_digit(u));
    }
    false
}

fn compute_kinds(c: &Classified) -> u16 {
    let mut kinds = 0u16;
    let is_git = c.prog_eq(b"git");
    if is_git
        && c.sub_in(&[b"status"])
        && !c.has_flag(b"s")
        && !c.has_flag(b"--short")
        && !c.has_flag(b"--porcelain")
    {
        kinds |= KIND_GIT_STATUS;
    }
    if is_git && c.sub_in(&[b"push", b"pull", b"fetch", b"clone"]) {
        kinds |= KIND_PROGRESS;
    }
    if is_git && c.sub_in(&[b"log", b"show"]) && !has_git_log_skip_flag(&c.text) {
        kinds |= KIND_GIT_LOG;
    }
    if is_git && c.sub_in(&[b"diff", b"show", b"log"]) {
        kinds |= KIND_GIT_DIFF;
    }
    if is_git && c.sub_in(&[b"branch"]) && !c.has_flag(b"v") && !c.has_flag(b"--verbose") {
        kinds |= KIND_GIT_BRANCH;
    }
    if c.prog_eq(b"ls") && c.has_flag(b"l") {
        kinds |= KIND_LS;
    }
    if c.prog_eq(b"wc") {
        kinds |= KIND_WC;
    }
    if c.prog_eq(b"rg")
        || ((c.prog_eq(b"grep") || c.prog_eq(b"egrep"))
            && (c.has_flag(b"n") || c.has_flag(b"--line-number"))
            && (c.has_flag(b"r")
                || c.has_flag(b"R")
                || c.has_flag(b"H")
                || c.has_flag(b"--recursive")))
    {
        kinds |= KIND_GREP;
    }

    let pkg_run_test = PKG_MANAGERS.iter().any(|&p| c.prog_eq(p))
        && c.sub_in(&[b"run", b"run-script"])
        && (1..c.text.len()).any(|i| {
            is_js_whitespace(c.text[i - 1]) && {
                let r = &c.text[i..];
                let after = if starts_with_ascii(r, b"run-script") {
                    10
                } else if starts_with_ascii(r, b"run") {
                    3
                } else {
                    0
                };
                after > 0 && r.get(after).is_some_and(|&u| is_js_whitespace(u)) && {
                    let mut k = after;
                    while k < r.len() && is_js_whitespace(r[k]) {
                        k += 1;
                    }
                    starts_with_ascii(&r[k..], b"test")
                }
            }
        });
    let sub_is_test = if c.prog_eq(b"go") || c.prog_eq(b"deno") || c.prog_eq(b"dotnet") {
        c.sub_in(&[b"test"])
    } else if PKG_MANAGERS.iter().any(|&p| c.prog_eq(p)) {
        c.sub_in(&[b"test", b"t"])
    } else if c.prog_eq(b"cargo") {
        c.sub_in(&[b"test", b"nextest"])
    } else if c.prog_eq(b"make") {
        contains_ascii_word(&c.sub, b"test") || contains_ascii_word(&c.sub, b"check")
    } else {
        false
    };
    if TEST_PROGRAMS.iter().any(|&p| c.prog_eq(p))
        || (c.prog_eq(b"node") && contains_ws_flag(&c.text, b"--test"))
        || pkg_run_test
        || sub_is_test
    {
        kinds |= KIND_TEST;
    }

    let pkg_install = c.sub_in(&[b"install", b"i", b"ci", b"add", b"update", b"upgrade"]);
    let sub_progress = if PKG_MANAGERS.iter().any(|&p| c.prog_eq(p)) {
        pkg_install
    } else if c.prog_eq(b"uv") {
        c.sub_in(&[b"pip", b"sync", b"add"])
    } else if c.prog_eq(b"poetry") {
        c.sub_in(&[b"install", b"add", b"update"])
    } else if c.prog_eq(b"cargo") {
        c.sub_in(&[
            b"build", b"check", b"run", b"test", b"nextest", b"clippy", b"install", b"update",
            b"fetch", b"doc",
        ])
    } else if c.prog_eq(b"go") {
        c.sub_in(&[
            b"build", b"test", b"get", b"mod", b"run", b"install", b"vet",
        ])
    } else if c.prog_eq(b"docker") {
        c.sub_in(&[b"build", b"pull", b"compose", b"buildx"])
    } else if c.prog_eq(b"bundle") || c.prog_eq(b"composer") {
        c.sub_in(&[b"install", b"update"])
    } else {
        false
    };
    if c.prog_eq(b"docker-compose") || is_pip_prog(&c.prog) || sub_progress {
        kinds |= KIND_PROGRESS;
    }
    kinds
}

/// Splits stripped command on `&&`, `||`, `;`, `\n`.
fn split_segments(s: &[u16]) -> Vec<&[u16]> {
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
            continue;
        }
        if matches!(s[i], 0x003B | 0x000A) {
            segs.push(&s[start..i]);
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    segs.push(&s[start..]);
    segs
}

#[must_use]
pub fn kinds_for(command: &[u16]) -> u16 {
    if trim(command).is_empty() {
        return 0;
    }
    let stripped = strip_quotes(command);
    let mut kinds = 0u16;
    for seg in split_segments(&stripped) {
        let t = trim(seg);
        if !t.is_empty()
            && let Some(c) = classify(t)
        {
            kinds |= c.kinds;
        }
    }
    kinds
}

#[must_use]
pub fn wrappable(segment: &[u16]) -> bool {
    let stripped = strip_quotes(segment);
    let Some(c) = classify(trim(&stripped)) else {
        return false;
    };
    let has_watch = (0..c.text.len()).any(|i| {
        starts_with_ascii(&c.text[i..], b"--watch")
            && (i + 7 == c.text.len() || !is_js_word(c.text[i + 7]))
    });
    if has_watch {
        return false;
    }
    if c.kinds & KIND_TEST != 0 {
        return true;
    }
    (c.kinds & KIND_PROGRESS != 0)
        && !c.prog_eq(b"git")
        && !c.prog_eq(b"docker")
        && !c.prog_eq(b"docker-compose")
        && !eq_ascii(&c.sub, b"run")
}

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

fn should_drop_line(kinds: u16, line: &[u16]) -> bool {
    let safe_allow = is_pipeline_safe(line);

    if kinds & KIND_GIT_STATUS != 0 && matches_git_status_hint(line) && !safe_allow {
        return true;
    }
    if kinds & KIND_PROGRESS != 0 && !safe_allow && matches_progress_drop(line) {
        return true;
    }
    if kinds & KIND_TEST != 0 && matches_test_drop(line) {
        return true;
    }
    false
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
    let Some(close) = after_open.iter().position(|&u| u == u16::from(b')')) else {
        return false;
    };
    trim(&after_open[close + 1..]).is_empty()
}

fn is_non_ws_run_at_least(s: &[u16], min_len: usize) -> bool {
    s.len() >= min_len && !is_js_whitespace(s[0])
}

fn matches_progress_drop(l: &[u16]) -> bool {
    // Git fetch/push/clone progress.
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
    let after_remote = no_remote.unwrap_or(l);
    for prefix in [
        &b"Enumerating objects:"[..],
        b"Counting objects:",
        b"Compressing objects:",
        b"Writing objects:",
        b"Receiving objects:",
        b"Unpacking objects:",
        b"Resolving deltas:",
    ] {
        if starts_with_ascii(after_remote, prefix) {
            return true;
        }
    }
    if starts_with_ascii(after_remote, b"Total ") {
        let mut k = 6;
        while k < after_remote.len() && is_ascii_digit(after_remote[k]) {
            k += 1;
        }
        if k > 6 && starts_with_ascii(&after_remote[k..], b" (delta") {
            return true;
        }
    }
    if no_remote.is_none()
        && (starts_with_ascii(l, b"Delta compression using")
            || starts_with_ascii(l, b"Updating files:")
            || starts_with_ascii(l, b"Checking out files:"))
    {
        return true;
    }

    // npm / pnpm
    {
        let mut k = 0;
        while k < l.len() && is_ascii_digit(l[k]) {
            k += 1;
        }
        if k > 0 {
            let r = &l[k..];
            if eq_ascii(r, b" package is looking for funding")
                || eq_ascii(r, b" packages are looking for funding")
                || eq_ascii(r, b" package are looking for funding")
                || eq_ascii(r, b" packages is looking for funding")
            {
                return true;
            }
        }
    }
    if eq_ascii(trim_start(l), b"run `npm fund` for details") {
        return true;
    }
    if let Some(r) = l.strip_prefix(&[
        b'P' as u16,
        b'r' as u16,
        b'o' as u16,
        b'g' as u16,
        b'r' as u16,
        b'e' as u16,
        b's' as u16,
        b's' as u16,
        b':' as u16,
        b' ' as u16,
        b'r' as u16,
        b'e' as u16,
        b's' as u16,
        b'o' as u16,
        b'l' as u16,
        b'v' as u16,
        b'e' as u16,
        b'd' as u16,
        b' ' as u16,
    ]) && !r.is_empty()
        && is_ascii_digit(r[0])
    {
        return true;
    }
    if l.len() >= 5 && l.iter().all(|&u| u == u16::from(b'+')) {
        return true;
    }

    // pip
    if starts_with_ascii(l, b"Requirement already satisfied: ") {
        return true;
    }
    {
        let ws = trim_start(l);
        if ws.first() == Some(&0x2501) && ws.iter().all(|&u| u == 0x2501) {
            return true;
        }
        for w in [
            &b"Collecting "[..],
            b"Downloading ",
            b"Using cached ",
            b"Obtaining ",
        ] {
            if starts_with_ascii(ws, w) && ws.get(w.len()).is_some_and(|&u| !is_js_whitespace(u)) {
                return true;
            }
        }
    }

    // cargo / go
    if !l.is_empty() && is_js_whitespace(l[0]) {
        let ws = trim_start(l);
        if eq_ascii(ws, b"Updating crates.io index") {
            return true;
        }
        for w in [
            &b"Compiling "[..],
            b"Checking ",
            b"Downloaded ",
            b"Downloading ",
            b"Fresh ",
            b"Locking ",
            b"Adding ",
            b"Blocking ",
        ] {
            if starts_with_ascii(ws, w) && ws.get(w.len()).is_some_and(|&u| !is_js_whitespace(u)) {
                return true;
            }
        }
    }
    if starts_with_ascii(l, b"go: downloading ")
        || starts_with_ascii(l, b"go: finding ")
        || starts_with_ascii(l, b"go: extracting ")
    {
        return true;
    }

    // docker pull / buildkit
    if l.len() >= 14
        && l[..12].iter().all(|&u| is_ascii_hex_lower(u))
        && starts_with_ascii(&l[12..], b": ")
    {
        let rest = &l[14..];
        for msg in [
            &b"Pulling fs layer"[..],
            b"Waiting",
            b"Verifying Checksum",
            b"Download complete",
            b"Pull complete",
            b"Already exists",
            b"Downloading",
            b"Extracting",
        ] {
            if starts_with_ascii(rest, msg) {
                return true;
            }
        }
    }
    if l.first() == Some(&u16::from(b'#')) {
        let mut k = 1;
        while k < l.len() && is_ascii_digit(l[k]) {
            k += 1;
        }
        if k > 1 && l.get(k) == Some(&u16::from(b' ')) {
            let r = &l[k + 1..];
            if starts_with_ascii(r, b"transferring ") || starts_with_ascii(r, b"extracting sha256:")
            {
                return true;
            }
            if starts_with_ascii(r, b"sha256:") {
                let mut h = 7;
                while h < r.len() && is_ascii_hex_lower(r[h]) {
                    h += 1;
                }
                if h > 7 && r.get(h) == Some(&u16::from(b' ')) {
                    let mut n = h + 1;
                    while n < r.len() && (is_ascii_digit(r[n]) || r[n] == u16::from(b'.')) {
                        n += 1;
                    }
                    if n > h + 1 {
                        if matches!(r.get(n), Some(0x006B | 0x004D | 0x0047)) {
                            n += 1;
                        }
                        if starts_with_ascii(&r[n..], b"B /") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn ends_with_pct_bracket(s: &[u16]) -> Option<usize> {
    if s.last() != Some(&u16::from(b']')) {
        return None;
    }
    let inner = &s[..s.len() - 1];
    if inner.last() != Some(&u16::from(b'%')) {
        return None;
    }
    let mut k = inner.len() - 1;
    let mut digits = 0;
    while k > 0 && is_ascii_digit(inner[k - 1]) {
        k -= 1;
        digits += 1;
    }
    if digits == 0 {
        return None;
    }
    while k > 0 && is_js_whitespace(inner[k - 1]) {
        k -= 1;
    }
    if k > 0 && inner[k - 1] == u16::from(b'[') {
        Some(k - 1)
    } else {
        None
    }
}

fn matches_test_drop(l: &[u16]) -> bool {
    // pytest -v / dots
    if !l.is_empty()
        && !is_js_whitespace(l[0])
        && let Some(pct_start) = ends_with_pct_bracket(l)
    {
        let before_pct = &l[..pct_start];
        if let Some(ws) = before_pct.iter().position(|&u| is_js_whitespace(u))
            && ws >= 4
            && eq_ascii(&before_pct[ws - 3..ws], b".py")
        {
            let mut m = ws;
            while m < before_pct.len() && is_js_whitespace(before_pct[m]) {
                m += 1;
            }
            let mid = &before_pct[m..];
            let dots_len = mid
                .iter()
                .take_while(|&&u| matches!(u, 0x002E | 0x0073 | 0x0078))
                .count();
            if dots_len > 0 && (dots_len..mid.len()).all(|k| is_js_whitespace(mid[k])) {
                return true;
            }
        }
        let body = trim_end(before_pct);
        if body.len() < before_pct.len()
            && body.len() >= 7
            && let Some(cc) = find_ascii(body, b"::")
            && cc > 0
            && !body[..cc].iter().any(|&u| is_js_whitespace(u))
            && cc + 2 < body.len()
            && !is_js_whitespace(body[cc + 2])
            && starts_with_ascii(&body[body.len() - 6..], b"PASSED")
            && is_js_whitespace(body[body.len() - 7])
        {
            return true;
        }
    }
    if let Some(cc) = find_ascii(l, b"::")
        && cc > 0
        && !l[..cc].iter().any(|&u| is_js_whitespace(u))
        && l.get(cc + 2).is_some_and(|&u| !is_js_whitespace(u))
        && l.len() >= 7
        && starts_with_ascii(&l[l.len() - 6..], b"PASSED")
        && is_js_whitespace(l[l.len() - 7])
    {
        return true;
    }

    let ws = trim_start(l);
    if !ws.is_empty()
        && matches!(ws[0], 0x2713 | 0x2714 | 0x221A | 0x25B6)
        && ws.get(1).is_some_and(|&u| is_js_whitespace(u))
    {
        return true;
    }
    if starts_with_ascii(ws, b"PASS") {
        let mut k = 4;
        if k < ws.len() && is_js_whitespace(ws[k]) {
            while k < ws.len() && is_js_whitespace(ws[k]) {
                k += 1;
            }
            if is_non_ws_run_at_least(&ws[k..], 1) {
                return true;
            }
        }
    }
    if starts_with_ascii(l, b"test ")
        && let Some(sp) = l[5..].iter().position(|&u| is_js_whitespace(u))
        && sp > 0
        && eq_ascii(&l[5 + sp..], b" ... ok")
    {
        return true;
    }
    if starts_with_ascii(ws, b"--- PASS: ") || starts_with_ascii(ws, b"# Subtest: ") {
        return true;
    }

    // cargo test scaffolding
    if !l.is_empty() && is_js_whitespace(l[0]) {
        if starts_with_ascii(ws, b"Finished `") {
            return true;
        }
        if starts_with_ascii(ws, b"Running ")
            && ws.get(8).is_some_and(|&u| !is_js_whitespace(u))
            && (find_ascii(ws, b" (target/").is_some() || find_ascii(ws, b" (target\\").is_some())
        {
            return true;
        }
        if starts_with_ascii(ws, b"Doc-tests ")
            && !ws[10..].is_empty()
            && !ws[10..].iter().any(|&u| is_js_whitespace(u))
        {
            return true;
        }
    }
    if starts_with_ascii(l, b"running ") {
        let mut k = 8;
        while k < l.len() && is_ascii_digit(l[k]) {
            k += 1;
        }
        if k > 8 && (eq_ascii(&l[k..], b" test") || eq_ascii(&l[k..], b" tests")) {
            return true;
        }
    }
    starts_with_ascii(l, b"test result: ok. 0 passed; 0 failed;")
        || starts_with_ascii(l, b"note: run with `RUST_BACKTRACE=1`")
}

fn go_verbose(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let has_run = lines.iter().any(|l| {
        starts_with_ascii(l, b"=== RUN") && l.get(7).is_some_and(|&u| is_js_whitespace(u))
    });
    if !has_run {
        return lines;
    }
    let mut keep: Vec<Vec<u16>> = Vec::new();
    for l in &lines {
        let ws = trim_start(l);
        let rest = starts_with_ascii(ws, b"--- FAIL: ")
            .then(|| &ws[10..])
            .or_else(|| starts_with_ascii(ws, b"--- SKIP: ").then(|| &ws[10..]));
        if let Some(r) = rest {
            let name: Vec<u16> = r
                .iter()
                .copied()
                .take_while(|&u| !is_js_whitespace(u))
                .collect();
            if !name.is_empty() && !keep.iter().any(|k| k == &name) {
                keep.push(name);
            }
        }
    }
    let mut out = Vec::with_capacity(lines.len());
    let mut cur: Option<Vec<u16>> = None;
    for l in lines {
        if starts_with_ascii(&l, b"=== ") {
            let mut parsed = None;
            for (tag, pause) in [
                (&b"RUN"[..], false),
                (b"PAUSE", true),
                (b"CONT", false),
                (b"NAME", false),
            ] {
                if starts_with_ascii(&l[4..], tag) {
                    let mut k = 4 + tag.len();
                    if k < l.len() && is_js_whitespace(l[k]) {
                        while k < l.len() && is_js_whitespace(l[k]) {
                            k += 1;
                        }
                        let name: Vec<u16> = l[k..]
                            .iter()
                            .copied()
                            .take_while(|&u| !is_js_whitespace(u))
                            .collect();
                        if !name.is_empty() {
                            parsed = Some((pause, name));
                        }
                    }
                    break;
                }
            }
            if let Some((is_pause, name)) = parsed {
                let kept = keep.iter().any(|k| k == &name);
                cur = Some(name);
                if kept && !is_pause {
                    out.push(l);
                }
                continue;
            }
        }
        let ws = trim_start(&l);
        if starts_with_ascii(ws, b"--- PASS: ")
            || starts_with_ascii(ws, b"--- FAIL: ")
            || starts_with_ascii(ws, b"--- SKIP: ")
        {
            cur = None;
        } else if let Some(ref c) = cur
            && !keep.iter().any(|k| k == c)
        {
            continue;
        }
        out.push(l);
    }
    out
}

fn drop_tap_passes(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let ws = trim_start(&lines[i]);
        let is_ok = starts_with_ascii(ws, b"ok ") && {
            let mut k = 3;
            while k < ws.len() && is_ascii_digit(ws[k]) {
                k += 1;
            }
            k > 3 && starts_with_ascii(&ws[k..], b" - ")
        };
        if is_ok {
            if let Some(next) = lines.get(i + 1)
                && eq_ascii(trim_start(next), b"---")
            {
                let mut j = i + 2;
                while j < lines.len() && !eq_ascii(trim_start(&lines[j]), b"...") {
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            i += 1;
            continue;
        }
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

fn is_four_space_word(l: &[u16]) -> bool {
    starts_with_ascii(l, b"    ") && l.len() > 4 && !l[4..].iter().any(|&u| is_js_whitespace(u))
}

fn compact_cargo(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        if eq_ascii(l, b"failures:") && lines.get(i + 1).is_some_and(|n| is_four_space_word(n)) {
            while lines.get(i + 1).is_some_and(|n| is_four_space_word(n)) {
                i += 1;
            }
            i += 1;
            continue;
        }
        if let Some(replaced) = rewrite_cargo_result_line(l) {
            out.push(replaced);
        } else {
            out.push(l.clone());
        }
        i += 1;
    }
    out
}

fn rewrite_cargo_result_line(l: &[u16]) -> Option<Vec<u16>> {
    if !starts_with_ascii(l, b"test result: ") {
        return None;
    }
    let mut k = 13;
    let w_start = k;
    while k < l.len() && is_js_word(l[k]) {
        k += 1;
    }
    if k == w_start || !starts_with_ascii(&l[k..], b". ") {
        return None;
    }
    let marker = b"; finished in ";
    let fin_pos = find_ascii(l, marker)?;
    let time = &l[fin_pos + marker.len()..];
    if time.is_empty() || time.iter().any(|&u| is_js_whitespace(u)) {
        return None;
    }
    let head = &l[..fin_pos];
    // Strip `; 0 [a-z ]+(?=;|$)` occurrences cleanly.
    let mut clean_head = Vec::with_capacity(head.len());
    let mut p = 0;
    while p < head.len() {
        if starts_with_ascii(&head[p..], b"; 0 ") {
            let mut q = p + 4;
            while q < head.len() && (matches!(head[q], 0x0061..=0x007A | 0x0020)) {
                q += 1;
            }
            if q > p + 4 && (q == head.len() || head[q] == u16::from(b';')) {
                p = q;
                continue;
            }
        }
        clean_head.push(head[p]);
        p += 1;
    }
    clean_head.push(u16::from(b' '));
    clean_head.push(u16::from(b'('));
    clean_head.extend_from_slice(time);
    clean_head.push(u16::from(b')'));
    Some(clean_head)
}

fn map_status_section(l: &[u16]) -> Option<u8> {
    if eq_ascii(l, b"Changes to be committed:") {
        Some(b'x')
    } else if eq_ascii(l, b"Changes not staged for commit:") {
        Some(b'y')
    } else if eq_ascii(l, b"Unmerged paths:") {
        Some(b'u')
    } else if eq_ascii(l, b"Untracked files:") {
        Some(b'?')
    } else if eq_ascii(l, b"Ignored files:") {
        Some(b'!')
    } else {
        None
    }
}

fn map_status_code(verb: &[u16], unmerged: bool) -> Option<&'static [u8]> {
    if unmerged {
        if eq_ascii(verb, b"both modified") {
            Some(b"UU")
        } else if eq_ascii(verb, b"added by us") {
            Some(b"AU")
        } else if eq_ascii(verb, b"added by them") {
            Some(b"UA")
        } else if eq_ascii(verb, b"deleted by us") {
            Some(b"DU")
        } else if eq_ascii(verb, b"deleted by them") {
            Some(b"UD")
        } else if eq_ascii(verb, b"both added") {
            Some(b"AA")
        } else if eq_ascii(verb, b"both deleted") {
            Some(b"DD")
        } else {
            None
        }
    } else if eq_ascii(verb, b"modified") {
        Some(b"M")
    } else if eq_ascii(verb, b"new file") {
        Some(b"A")
    } else if eq_ascii(verb, b"deleted") {
        Some(b"D")
    } else if eq_ascii(verb, b"renamed") {
        Some(b"R")
    } else if eq_ascii(verb, b"copied") {
        Some(b"C")
    } else if eq_ascii(verb, b"typechange") {
        Some(b"T")
    } else {
        None
    }
}

fn upsert_ordered(map: &mut Vec<(Vec<u16>, Vec<u16>)>, key: Vec<u16>, val: Vec<u16>) {
    if let Some((_, existing)) = map.iter_mut().find(|(k, _)| k == &key) {
        *existing = val;
    } else {
        map.push((key, val));
    }
}

fn short_status(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::with_capacity(lines.len());
    let mut files: Vec<(Vec<u16>, Vec<u16>)> = Vec::new();
    let mut sec: Option<u8> = None;
    let mut at: Option<usize> = None;

    for l in lines {
        if let Some(s) = map_status_section(&l) {
            sec = Some(s);
            if at.is_none() {
                at = Some(out.len());
            }
            continue;
        }
        if trim(&l).is_empty() {
            sec = None;
            continue;
        }
        if let Some(s) = sec
            && l.first() == Some(&0x0009)
        {
            let e = &l[1..];
            if s == b'?' || s == b'!' {
                let prefix = if s == b'?' { "??" } else { "!!" };
                let mut key = Vec::with_capacity(2 + e.len());
                push_ascii(&mut key, prefix);
                key.extend_from_slice(e);
                let mut val = Vec::with_capacity(3 + e.len());
                push_ascii(&mut val, prefix);
                val.push(u16::from(b' '));
                val.extend_from_slice(e);
                upsert_ordered(&mut files, key, val);
                continue;
            }
            if let Some(colon) = e.iter().position(|&u| u == u16::from(b':'))
                && e[..colon]
                    .iter()
                    .all(|&u| matches!(u, 0x0061..=0x007A | 0x0020))
                && colon > 0
                && e.get(colon + 1).is_some_and(|&u| is_js_whitespace(u))
            {
                let verb = &e[..colon];
                let mut rest_idx = colon + 1;
                while rest_idx < e.len() && is_js_whitespace(e[rest_idx]) {
                    rest_idx += 1;
                }
                let file = &e[rest_idx..];
                if let Some(code) = map_status_code(verb, s == b'u') {
                    let mut val = Vec::with_capacity(3 + file.len());
                    if s == b'u' {
                        val.extend(code.iter().map(|&b| u16::from(b)));
                        val.push(u16::from(b' '));
                        val.extend_from_slice(file);
                    } else {
                        let prev = files
                            .iter()
                            .find(|(k, _)| k.as_slice() == file)
                            .map(|(_, v)| v.as_slice());
                        let x = if s == b'x' {
                            u16::from(code[0])
                        } else {
                            prev.and_then(|p| p.first().copied())
                                .unwrap_or(u16::from(b' '))
                        };
                        let y = if s == b'y' {
                            u16::from(code[0])
                        } else {
                            u16::from(b' ')
                        };
                        val.push(x);
                        val.push(y);
                        val.push(u16::from(b' '));
                        val.extend_from_slice(file);
                    }
                    upsert_ordered(&mut files, file.to_vec(), val);
                    continue;
                }
            }
        }
        if starts_with_ascii(&l, b"no changes added to commit")
            || starts_with_ascii(&l, b"nothing added to commit but untracked files present")
        {
            continue;
        }
        out.push(l);
    }

    if let Some(pos) = at {
        let vals = files.into_iter().map(|(_, v)| v);
        out.splice(pos..pos, vals);
    }

    let refs: Vec<&[u16]> = out.iter().map(Vec::as_slice).collect();
    let mut text = join_u16(&refs, 0x000A);
    text = rewrite_branch_header_up_to_date(text);
    text = rewrite_branch_header_ahead_behind(text);
    text = rewrite_branch_header_diverged(text);
    text = rewrite_branch_header_plain(text);
    split_lf(&text).into_iter().map(|s| s.to_vec()).collect()
}

fn parse_until_single_quote(after_open: &[u16]) -> Option<(&[u16], &[u16])> {
    let q2 = after_open.iter().position(|&u| u == u16::from(b'\''))?;
    Some((&after_open[..q2], &after_open[q2 + 1..]))
}

fn rewrite_branch_header_up_to_date(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let line_end = text[pos..]
            .iter()
            .position(|&u| u == 0x000A)
            .map_or(text.len(), |x| pos + x);
        let l1 = &text[pos..line_end];
        if let Some(branch) = l1.strip_prefix(&[
            b'O' as u16,
            b'n' as u16,
            b' ' as u16,
            b'b' as u16,
            b'r' as u16,
            b'a' as u16,
            b'n' as u16,
            b'c' as u16,
            b'h' as u16,
            b' ' as u16,
        ]) && !branch.is_empty()
            && !branch.iter().any(|&u| is_js_whitespace(u))
            && line_end < text.len()
        {
            let l2_start = line_end + 1;
            let l2_end = text[l2_start..]
                .iter()
                .position(|&u| is_line_terminator(u))
                .map_or(text.len(), |x| l2_start + x);
            let l2 = &text[l2_start..l2_end];
            const UP_PREFIX: &[u8] = b"Your branch is up to date with '";
            if starts_with_ascii(l2, UP_PREFIX)
                && let Some((up, rest)) = parse_until_single_quote(&l2[UP_PREFIX.len()..])
                && !up.is_empty()
                && eq_ascii(rest, b".")
            {
                let mut out = text[..pos].to_vec();
                push_ascii(&mut out, "* ");
                out.extend_from_slice(branch);
                push_ascii(&mut out, "...");
                out.extend_from_slice(up);
                out.extend_from_slice(&text[l2_end..]);
                return out;
            }
        }
        if line_end == text.len() {
            break;
        }
        pos = line_end + 1;
    }
    text
}

fn rewrite_branch_header_ahead_behind(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let line_end = text[pos..]
            .iter()
            .position(|&u| u == 0x000A)
            .map_or(text.len(), |x| pos + x);
        let l1 = &text[pos..line_end];
        if starts_with_ascii(l1, b"On branch ")
            && l1.len() > 10
            && !l1[10..].iter().any(|&u| is_js_whitespace(u))
            && line_end < text.len()
        {
            let branch = &l1[10..];
            let l2_start = line_end + 1;
            let l2_end = text[l2_start..]
                .iter()
                .position(|&u| is_line_terminator(u))
                .map_or(text.len(), |x| l2_start + x);
            let l2 = &text[l2_start..l2_end];
            const AHEAD_PREFIX: &[u8] = b"Your branch is ahead of '";
            const BEHIND_PREFIX: &[u8] = b"Your branch is behind '";
            let dir_pair = starts_with_ascii(l2, AHEAD_PREFIX)
                .then(|| ("ahead", &l2[AHEAD_PREFIX.len()..]))
                .or_else(|| {
                    starts_with_ascii(l2, BEHIND_PREFIX)
                        .then(|| ("behind", &l2[BEHIND_PREFIX.len()..]))
                });
            if let Some((dir, after_quote)) = dir_pair
                && let Some((up, rest)) = parse_until_single_quote(after_quote)
                && !up.is_empty()
                && starts_with_ascii(rest, b" by ")
            {
                let after_by = &rest[4..];
                let digits = after_by.iter().take_while(|&&u| is_ascii_digit(u)).count();
                if digits > 0 {
                    let tail = &after_by[digits..];
                    if eq_ascii(tail, b" commit.")
                        || eq_ascii(tail, b" commits.")
                        || eq_ascii(tail, b" commit, and can be fast-forwarded.")
                        || eq_ascii(tail, b" commits, and can be fast-forwarded.")
                    {
                        let mut out = text[..pos].to_vec();
                        push_ascii(&mut out, "* ");
                        out.extend_from_slice(branch);
                        push_ascii(&mut out, "...");
                        out.extend_from_slice(up);
                        push_ascii(&mut out, " [");
                        push_ascii(&mut out, dir);
                        out.push(u16::from(b' '));
                        out.extend_from_slice(&after_by[..digits]);
                        out.push(u16::from(b']'));
                        out.extend_from_slice(&text[l2_end..]);
                        return out;
                    }
                }
            }
        }
        if line_end == text.len() {
            break;
        }
        pos = line_end + 1;
    }
    text
}

fn rewrite_branch_header_diverged(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let l1_end = text[pos..]
            .iter()
            .position(|&u| u == 0x000A)
            .map_or(text.len(), |x| pos + x);
        let l1 = &text[pos..l1_end];
        if starts_with_ascii(l1, b"On branch ")
            && l1.len() > 10
            && !l1[10..].iter().any(|&u| is_js_whitespace(u))
            && l1_end < text.len()
        {
            let branch = &l1[10..];
            let l2_start = l1_end + 1;
            let l2_end = text[l2_start..]
                .iter()
                .position(|&u| u == 0x000A)
                .map_or(text.len(), |x| l2_start + x);
            let l2 = &text[l2_start..l2_end];
            const DIV_PREFIX: &[u8] = b"Your branch and '";
            if starts_with_ascii(l2, DIV_PREFIX)
                && l2_end < text.len()
                && let Some((up, rest)) = parse_until_single_quote(&l2[DIV_PREFIX.len()..])
                && !up.is_empty()
                && eq_ascii(rest, b" have diverged,")
            {
                let l3_start = l2_end + 1;
                let l3_end = text[l3_start..]
                    .iter()
                    .position(|&u| is_line_terminator(u))
                    .map_or(text.len(), |x| l3_start + x);
                let l3 = &text[l3_start..l3_end];
                if let Some(after_have) = starts_with_ascii(l3, b"and have ").then(|| &l3[9..]) {
                    let d1 = after_have
                        .iter()
                        .take_while(|&&u| is_ascii_digit(u))
                        .count();
                    if d1 > 0 && starts_with_ascii(&after_have[d1..], b" and ") {
                        let after_and = &after_have[d1 + 5..];
                        let d2 = after_and.iter().take_while(|&&u| is_ascii_digit(u)).count();
                        if d2 > 0
                            && eq_ascii(&after_and[d2..], b" different commits each, respectively.")
                        {
                            let mut out = text[..pos].to_vec();
                            push_ascii(&mut out, "* ");
                            out.extend_from_slice(branch);
                            push_ascii(&mut out, "...");
                            out.extend_from_slice(up);
                            push_ascii(&mut out, " [ahead ");
                            out.extend_from_slice(&after_have[..d1]);
                            push_ascii(&mut out, ", behind ");
                            out.extend_from_slice(&after_and[..d2]);
                            out.push(u16::from(b']'));
                            out.extend_from_slice(&text[l3_end..]);
                            return out;
                        }
                    }
                }
            }
        }
        if l1_end == text.len() {
            break;
        }
        pos = l1_end + 1;
    }
    text
}

fn rewrite_branch_header_plain(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let end = text[pos..]
            .iter()
            .position(|&u| is_line_terminator(u))
            .map_or(text.len(), |x| pos + x);
        let line = &text[pos..end];
        if starts_with_ascii(line, b"On branch ")
            && line.len() > 10
            && !line[10..].iter().any(|&u| is_js_whitespace(u))
        {
            let mut out = text[..pos].to_vec();
            push_ascii(&mut out, "* ");
            out.extend_from_slice(&line[10..]);
            out.extend_from_slice(&text[end..]);
            return out;
        }
        if end == text.len() {
            break;
        }
        pos = end + 1;
    }
    text
}

fn compact_diff(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        let Some((_a, b_path)) = parse_diff_git_header(l) else {
            out.push(l.clone());
            i += 1;
            continue;
        };
        let mut path = b_path.to_vec();
        let mut notes: Vec<Vec<u16>> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() {
            let h = &lines[j];
            if is_diff_index_line(h)
                || starts_with_ascii(h, b"similarity index ")
                || starts_with_ascii(h, b"dissimilarity index ")
            {
                j += 1;
                continue;
            }
            if starts_with_ascii_digits_after(h, b"new file mode ") {
                notes.push(b"new".iter().map(|&x| u16::from(x)).collect());
                j += 1;
                continue;
            }
            if starts_with_ascii_digits_after(h, b"deleted file mode ") {
                notes.push(b"deleted".iter().map(|&x| u16::from(x)).collect());
                j += 1;
                continue;
            }
            if let Some(m) = starts_with_digits_slice(h, b"old mode ") {
                let mut note = Vec::new();
                push_ascii(&mut note, "mode ");
                note.extend_from_slice(m);
                notes.push(note);
                j += 1;
                continue;
            }
            if let Some(m) = starts_with_digits_slice(h, b"new mode ") {
                let mut note = Vec::new();
                push_ascii(&mut note, "-> ");
                note.extend_from_slice(m);
                notes.push(note);
                j += 1;
                continue;
            }
            if let Some(from) = starts_with_ascii(h, b"rename from ")
                .then(|| &h[12..])
                .or_else(|| starts_with_ascii(h, b"copy from ").then(|| &h[10..]))
            {
                let mut renamed = Vec::with_capacity(from.len() + 4 + path.len());
                renamed.extend_from_slice(from);
                push_ascii(&mut renamed, " -> ");
                renamed.extend_from_slice(&path);
                path = renamed;
                j += 1;
                continue;
            }
            if starts_with_ascii(h, b"rename to ") || starts_with_ascii(h, b"copy to ") {
                j += 1;
                continue;
            }
            if starts_with_ascii(h, b"--- a/") || eq_ascii(h, b"--- /dev/null") {
                j += 1;
                continue;
            }
            if starts_with_ascii(h, b"+++ b/") {
                let new_p = &h[6..];
                if !new_p.is_empty() && find_ascii(&path, b" -> ").is_none() {
                    path = new_p.to_vec();
                }
                j += 1;
                continue;
            }
            if eq_ascii(h, b"+++ /dev/null") {
                j += 1;
                continue;
            }
            break;
        }
        if !notes.is_empty() {
            push_ascii(&mut path, " (");
            for (k, n) in notes.iter().enumerate() {
                if k > 0 {
                    path.push(u16::from(b' '));
                }
                path.extend_from_slice(n);
            }
            path.push(u16::from(b')'));
        }
        out.push(path);
        i = j;
    }
    out
}

fn parse_diff_git_header(l: &[u16]) -> Option<(&[u16], &[u16])> {
    let rest = starts_with_ascii(l, b"diff --git a/").then(|| &l[13..])?;
    // Greedy `(.*)` then ` b/(.*)`: match the LAST ` b/` occurrence.
    for k in (0..=rest.len().saturating_sub(3)).rev() {
        if starts_with_ascii(&rest[k..], b" b/") {
            return Some((&rest[..k], &rest[k + 3..]));
        }
    }
    None
}

fn is_diff_index_line(h: &[u16]) -> bool {
    if !starts_with_ascii(h, b"index ") {
        return false;
    }
    let rest = &h[6..];
    let left = rest.iter().take_while(|&&u| is_ascii_hex_lower(u)).count();
    if left == 0 || !starts_with_ascii(&rest[left..], b"..") {
        return false;
    }
    let after_dots = &rest[left + 2..];
    after_dots.first().is_some_and(|&u| is_ascii_hex_lower(u))
}

fn starts_with_ascii_digits_after(h: &[u16], prefix: &[u8]) -> bool {
    starts_with_ascii(h, prefix) && h.get(prefix.len()).is_some_and(|&u| is_ascii_digit(u))
}

fn starts_with_digits_slice<'a>(h: &'a [u16], prefix: &[u8]) -> Option<&'a [u16]> {
    if !starts_with_ascii(h, prefix) {
        return None;
    }
    let rest = &h[prefix.len()..];
    let n = rest.iter().take_while(|&&u| is_ascii_digit(u)).count();
    (n > 0).then(|| &rest[..n])
}

fn parse_stat_line(line: &[u16]) -> Option<(&[u16], &[u16])> {
    if line.first() != Some(&u16::from(b' ')) || line.get(1).is_none_or(|&u| is_js_whitespace(u)) {
        return None;
    }
    // Non-greedy `(\S.*?)`: check earliest valid `\s+\|\s+` split.
    let body = &line[1..];
    for pos in 1..body.len() {
        if !is_js_whitespace(body[pos]) {
            continue;
        }
        let mut ws1_end = pos;
        while ws1_end < body.len() && is_js_whitespace(body[ws1_end]) {
            ws1_end += 1;
        }
        if body.get(ws1_end) != Some(&u16::from(b'|')) {
            continue;
        }
        let mut ws2_end = ws1_end + 1;
        while ws2_end < body.len() && is_js_whitespace(body[ws2_end]) {
            ws2_end += 1;
        }
        if ws2_end == ws1_end + 1 {
            continue;
        }
        let rhs = &body[ws2_end..];
        if starts_with_ascii(rhs, b"Bin ") {
            return Some((&body[..pos], rhs));
        }
        let digits = rhs.iter().take_while(|&&u| is_ascii_digit(u)).count();
        if digits > 0 {
            let after_digits = &rhs[digits..];
            let tail = after_digits
                .strip_prefix(&[u16::from(b' ')])
                .unwrap_or(after_digits);
            if tail
                .iter()
                .all(|&u| u == u16::from(b'+') || u == u16::from(b'-'))
            {
                return Some((&body[..pos], &rhs[..digits]));
            }
        }
    }
    None
}

type StatSummary<'a> = (&'a [u16], Option<&'a [u16]>, Option<&'a [u16]>);

fn parse_stat_summary(line: &[u16]) -> Option<StatSummary<'_>> {
    if line.first() != Some(&u16::from(b' ')) {
        return None;
    }
    let after_sp = &line[1..];
    let n_files = after_sp.iter().take_while(|&&u| is_ascii_digit(u)).count();
    if n_files == 0 {
        return None;
    }
    let files = &after_sp[..n_files];
    let after_num = &after_sp[n_files..];
    let mut rest = if starts_with_ascii(after_num, b" files changed") {
        &after_num[14..]
    } else if starts_with_ascii(after_num, b" file changed") {
        &after_num[13..]
    } else {
        return None;
    };

    let mut ins: Option<&[u16]> = None;
    let mut del: Option<&[u16]> = None;

    if starts_with_ascii(rest, b", ") {
        let d_slice = &rest[2..];
        let d_len = d_slice.iter().take_while(|&&u| is_ascii_digit(u)).count();
        if d_len > 0 {
            let after_d = &d_slice[d_len..];
            let ins_tail = starts_with_ascii(after_d, b" insertions(+)")
                .then(|| &after_d[14..])
                .or_else(|| starts_with_ascii(after_d, b" insertion(+)").then(|| &after_d[13..]));
            if let Some(t) = ins_tail {
                ins = Some(&d_slice[..d_len]);
                rest = t;
            }
        }
    }

    if !rest.is_empty() {
        let after_comma = starts_with_ascii(rest, b", ").then(|| &rest[2..])?;
        let d_len = after_comma
            .iter()
            .take_while(|&&u| is_ascii_digit(u))
            .count();
        if d_len == 0 {
            return None;
        }
        let after_d = &after_comma[d_len..];
        if eq_ascii(after_d, b" deletions(-)") || eq_ascii(after_d, b" deletion(-)") {
            del = Some(&after_comma[..d_len]);
        } else {
            return None;
        }
    }

    Some((files, ins, del))
}

fn compact_stat(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = lines.clone();
    for i in 0..lines.len() {
        let Some((files, ins, del)) = parse_stat_summary(&lines[i]) else {
            continue;
        };
        let mut summary = files.to_vec();
        push_ascii(
            &mut summary,
            if eq_ascii(files, b"1") {
                " file"
            } else {
                " files"
            },
        );
        if let Some(ins_n) = ins {
            push_ascii(&mut summary, " +");
            summary.extend_from_slice(ins_n);
        }
        if let Some(del_n) = del {
            push_ascii(&mut summary, " -");
            summary.extend_from_slice(del_n);
        }
        out[i] = summary;

        let mut j = i;
        while j > 0 {
            j -= 1;
            let Some((path, stat)) = parse_stat_line(&lines[j]) else {
                break;
            };
            let mut row = path.to_vec();
            push_ascii(&mut row, " | ");
            row.extend_from_slice(stat);
            out[j] = row;
        }
    }
    out
}

fn is_commit_header(l: &[u16]) -> Option<(&[u16], &[u16])> {
    if !starts_with_ascii(l, b"commit ") {
        return None;
    }
    let rest = &l[7..];
    if rest.len() >= 40 && rest[..40].iter().all(|&u| is_ascii_hex_lower(u)) {
        Some((&rest[..40], &rest[40..]))
    } else {
        None
    }
}

fn strip_author_email(raw: &[u16]) -> &[u16] {
    if raw.last() != Some(&u16::from(b'>')) {
        return raw;
    }
    if let Some(open) = raw.iter().rposition(|&u| u == u16::from(b'<'))
        && !raw[open + 1..raw.len() - 1].contains(&u16::from(b'>'))
    {
        let mut k = open;
        while k > 0 && is_js_whitespace(raw[k - 1]) {
            k -= 1;
        }
        return &raw[..k];
    }
    raw
}

fn month_num(m: &[u16]) -> Option<&'static str> {
    if eq_ascii(m, b"Jan") {
        Some("01")
    } else if eq_ascii(m, b"Feb") {
        Some("02")
    } else if eq_ascii(m, b"Mar") {
        Some("03")
    } else if eq_ascii(m, b"Apr") {
        Some("04")
    } else if eq_ascii(m, b"May") {
        Some("05")
    } else if eq_ascii(m, b"Jun") {
        Some("06")
    } else if eq_ascii(m, b"Jul") {
        Some("07")
    } else if eq_ascii(m, b"Aug") {
        Some("08")
    } else if eq_ascii(m, b"Sep") {
        Some("09")
    } else if eq_ascii(m, b"Oct") {
        Some("10")
    } else if eq_ascii(m, b"Nov") {
        Some("11")
    } else if eq_ascii(m, b"Dec") {
        Some("12")
    } else {
        None
    }
}

fn short_date(raw: &[u16]) -> Vec<u16> {
    // /^\w{3} (\w{3}) (\d{1,2}) [\d:]+ (\d{4}) [+-]\d{4}$/
    if raw.len() >= 20
        && raw[..3].iter().all(|&u| is_js_word(u))
        && raw[3] == u16::from(b' ')
        && raw[4..7].iter().all(|&u| is_js_word(u))
        && raw[7] == u16::from(b' ')
        && let Some(mon) = month_num(&raw[4..7])
    {
        let mut k = 8;
        let d_start = k;
        while k < raw.len() && is_ascii_digit(raw[k]) && k - d_start < 2 {
            k += 1;
        }
        let day = &raw[d_start..k];
        if !day.is_empty() && raw.get(k) == Some(&u16::from(b' ')) {
            k += 1;
            let t_start = k;
            while k < raw.len() && (is_ascii_digit(raw[k]) || raw[k] == u16::from(b':')) {
                k += 1;
            }
            if k > t_start && raw.get(k) == Some(&u16::from(b' ')) {
                k += 1;
                if raw.len() == k + 10
                    && raw[k..k + 4].iter().all(|&u| is_ascii_digit(u))
                    && raw[k + 4] == u16::from(b' ')
                    && matches!(raw[k + 5], 0x002B | 0x002D)
                    && raw[k + 6..k + 10].iter().all(|&u| is_ascii_digit(u))
                {
                    let mut out = raw[k..k + 4].to_vec();
                    out.push(u16::from(b'-'));
                    push_ascii(&mut out, mon);
                    out.push(u16::from(b'-'));
                    if day.len() == 1 {
                        out.push(u16::from(b'0'));
                    }
                    out.extend_from_slice(day);
                    return out;
                }
            }
        }
    }
    raw.to_vec()
}

fn compact_git_log(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    const BODY_CAP: usize = 3;
    let commits = lines
        .iter()
        .filter(|l| is_commit_header(l).is_some())
        .count();
    let many = commits > 1;

    let mut authors: Vec<Vec<u16>> = Vec::new();
    for l in &lines {
        if starts_with_ascii(l, b"Author:") && l.get(7).is_some_and(|&u| is_js_whitespace(u)) {
            let mut k = 7;
            while k < l.len() && is_js_whitespace(l[k]) {
                k += 1;
            }
            let a = strip_author_email(&l[k..]).to_vec();
            if !authors.iter().any(|x| x == &a) {
                authors.push(a);
            }
        }
    }
    let one_author: Option<&[u16]> = if many && authors.len() == 1 {
        Some(&authors[0])
    } else {
        None
    };

    let mut out = Vec::with_capacity(lines.len());
    if let Some(a) = one_author {
        let mut h = Vec::new();
        push_ascii(&mut h, "(all by ");
        h.extend_from_slice(a);
        h.push(u16::from(b')'));
        out.push(h);
    }

    let mut i = 0;
    while i < lines.len() {
        let Some((hash, rest_hdr)) = is_commit_header(&lines[i]) else {
            out.push(lines[i].clone());
            i += 1;
            continue;
        };
        let mut meta = Vec::new();
        let mut merge = Vec::new();
        let mut j = i + 1;
        while j < lines.len() {
            let line = &lines[j];
            let parsed = if starts_with_ascii(line, b"Merge:") {
                Some((0u8, &line[6..]))
            } else if starts_with_ascii(line, b"Author:") {
                Some((1u8, &line[7..]))
            } else if starts_with_ascii(line, b"Date:") {
                Some((2u8, &line[5..]))
            } else {
                None
            };
            let Some((kind, after_colon)) = parsed else {
                break;
            };
            let mut k = 0;
            while k < after_colon.len() && is_js_whitespace(after_colon[k]) {
                k += 1;
            }
            if k == 0 {
                break;
            }
            let val = &after_colon[k..];
            match kind {
                0 => {
                    merge.clear();
                    push_ascii(&mut merge, " merge ");
                    merge.extend_from_slice(val);
                }
                1 => {
                    if one_author.is_none() {
                        meta.push(u16::from(b' '));
                        meta.extend_from_slice(strip_author_email(val));
                    }
                }
                _ => {
                    let mut prefix = vec![u16::from(b' ')];
                    prefix.extend_from_slice(&short_date(val));
                    prefix.extend_from_slice(&meta);
                    meta = prefix;
                }
            }
            j += 1;
        }

        let mut b_idx = if lines.get(j).is_some_and(|l| l.is_empty()) {
            j + 1
        } else {
            j
        };
        let mut body: Vec<&[u16]> = Vec::new();
        while b_idx < lines.len()
            && (starts_with_ascii(&lines[b_idx], b"    ") || trim(&lines[b_idx]).is_empty())
        {
            if !trim(&lines[b_idx]).is_empty() {
                body.push(&lines[b_idx][4..]);
            } else if b_idx + 1 < lines.len() && !starts_with_ascii(&lines[b_idx + 1], b"    ") {
                b_idx += 1;
                break;
            }
            b_idx += 1;
        }

        let subject = if body.is_empty() { &[][..] } else { body[0] };
        let body_rest = if body.is_empty() { &[][..] } else { &body[1..] };

        let mut hdr = hash[..10].to_vec();
        hdr.extend_from_slice(rest_hdr);
        hdr.extend_from_slice(&merge);
        hdr.push(u16::from(b' '));
        hdr.extend_from_slice(subject);
        push_ascii(&mut hdr, " (");
        hdr.extend_from_slice(trim(&meta));
        hdr.push(u16::from(b')'));
        out.push(hdr);

        let keep_n = if many {
            body_rest.len().min(BODY_CAP)
        } else {
            body_rest.len()
        };
        for b in &body_rest[..keep_n] {
            let mut indented = vec![u16::from(b' '), u16::from(b' ')];
            indented.extend_from_slice(b);
            out.push(indented);
        }
        if keep_n < body_rest.len() {
            let mut more = Vec::new();
            push_ascii(&mut more, "  [+");
            push_usize(&mut more, body_rest.len() - keep_n);
            push_ascii(&mut more, " lines]");
            out.push(more);
        }

        let mut p = b_idx;
        while p < lines.len() && is_commit_header(&lines[p]).is_none() {
            if !(many && trim(&lines[p]).is_empty()) {
                out.push(lines[p].clone());
            }
            p += 1;
        }
        i = p;
    }
    out
}

fn parse_branch_ref(line: &[u16], all_remote: bool) -> Option<(&[u16], &[u16])> {
    let after_sp = if all_remote {
        starts_with_ascii(line, b"  ").then(|| &line[2..])?
    } else {
        starts_with_ascii(line, b"  remotes/").then(|| &line[10..])?
    };
    let slash = after_sp.iter().position(|&u| u == u16::from(b'/'))?;
    if slash == 0 || after_sp[..slash].iter().any(|&u| is_js_whitespace(u)) {
        return None;
    }
    let remote = &after_sp[..slash];
    let rest = &after_sp[slash + 1..];
    // Reject empty or multi-space unless exact `S+ -> S+`.
    if rest.is_empty() {
        return None;
    }
    let valid_ref = if !rest.iter().any(|&u| is_js_whitespace(u)) {
        true
    } else if let Some(arrow) = find_ascii(rest, b" -> ") {
        let lhs = &rest[..arrow];
        let rhs = &rest[arrow + 4..];
        !lhs.is_empty()
            && !rhs.is_empty()
            && !lhs.iter().any(|&u| is_js_whitespace(u))
            && !rhs.iter().any(|&u| is_js_whitespace(u))
    } else {
        false
    };
    valid_ref.then_some((remote, rest))
}

fn compact_branches(lines: Vec<Vec<u16>>, all_remote: bool) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::with_capacity(lines.len());
    let mut remotes: Vec<(Vec<u16>, Vec<Vec<u16>>)> = Vec::new();

    for l in lines {
        let Some((remote, branch_spec)) = parse_branch_ref(&l, all_remote) else {
            out.push(l);
            continue;
        };
        let mut arrow_remote = b" -> ".iter().map(|&x| u16::from(x)).collect::<Vec<u16>>();
        arrow_remote.extend_from_slice(remote);
        arrow_remote.push(u16::from(b'/'));

        let cleaned = if let Some(pos) = find_subslice(branch_spec, &arrow_remote) {
            let mut c = branch_spec[..pos].to_vec();
            push_ascii(&mut c, " -> ");
            c.extend_from_slice(&branch_spec[pos + arrow_remote.len()..]);
            c
        } else {
            branch_spec.to_vec()
        };

        if let Some((_, list)) = remotes.iter_mut().find(|(r, _)| r.as_slice() == remote) {
            list.push(cleaned);
        } else {
            out.push(remote.to_vec());
            remotes.push((remote.to_vec(), vec![cleaned]));
        }
    }

    out.into_iter()
        .map(|l| {
            if l.first() != Some(&u16::from(b' '))
                && let Some((_, list)) = remotes.iter().find(|(r, _)| r == &l)
            {
                let mut row = l;
                push_ascii(&mut row, "/: ");
                for (idx, b) in list.iter().enumerate() {
                    if idx > 0 {
                        push_ascii(&mut row, ", ");
                    }
                    row.extend_from_slice(b);
                }
                return row;
            }
            l
        })
        .collect()
}

struct LsEntry<'a> {
    ftype: u16,
    perm: &'a [u16],
    mark: Option<u16>,
    owner: &'a [u16],
    group: &'a [u16],
    size: &'a [u16],
    date: &'a [u16],
    name: &'a [u16],
}

fn parse_ls_date(s: &[u16]) -> Option<(&[u16], &[u16])> {
    // Variant 1: YYYY-MM-DD HH:MM:SS(.nnn) +ZZZZ | YYYY-MM-DD HH:MM(:SS) | YYYY-MM-DD
    if s.len() >= 10
        && s[..4].iter().all(|&u| is_ascii_digit(u))
        && s[4] == u16::from(b'-')
        && s[5..7].iter().all(|&u| is_ascii_digit(u))
        && s[7] == u16::from(b'-')
        && s[8..10].iter().all(|&u| is_ascii_digit(u))
    {
        if s.get(10) == Some(&u16::from(b' '))
            && s.get(11)
                .is_some_and(|&u| is_ascii_digit(u) || matches!(u, 0x003A | 0x002E))
        {
            let mut k = 11;
            while k < s.len() && (is_ascii_digit(s[k]) || matches!(s[k], 0x003A | 0x002E)) {
                k += 1;
            }
            if s.get(k) == Some(&u16::from(b' '))
                && s.len() >= k + 7
                && matches!(s[k + 1], 0x002B | 0x002D)
                && s[k + 2..k + 6].iter().all(|&u| is_ascii_digit(u))
                && is_js_whitespace(s[k + 6])
                && s.len() > k + 7
            {
                return Some((&s[..k + 6], &s[k + 7..]));
            }
            if s.get(k).is_some_and(|&u| is_js_whitespace(u)) && s.len() > k + 1 {
                return Some((&s[..k], &s[k + 1..]));
            }
        }
        if s.get(10).is_some_and(|&u| is_js_whitespace(u)) && s.len() > 11 {
            return Some((&s[..10], &s[11..]));
        }
    }

    // Variant 2: \w{3}\s+\d{1,2}\s+(?:\d{1,2}:\d{2}|\d{4})\s(.+)
    if s.len() >= 12 && s[..3].iter().all(|&u| is_js_word(u)) && is_js_whitespace(s[3]) {
        let mut k = 3;
        while k < s.len() && is_js_whitespace(s[k]) {
            k += 1;
        }
        let d_start = k;
        while k < s.len() && is_ascii_digit(s[k]) && k - d_start < 2 {
            k += 1;
        }
        if k > d_start && k < s.len() && is_js_whitespace(s[k]) {
            while k < s.len() && is_js_whitespace(s[k]) {
                k += 1;
            }
            let t_start = k;
            while k < s.len() && is_ascii_digit(s[k]) && k - t_start < 4 {
                k += 1;
            }
            let dlen = k - t_start;
            let time_end = if (1..=2).contains(&dlen) && s.get(k) == Some(&u16::from(b':')) {
                if s.get(k + 1..k + 3)
                    .is_some_and(|m| m.iter().all(|&u| is_ascii_digit(u)))
                {
                    Some(k + 3)
                } else {
                    None
                }
            } else if dlen == 4 {
                Some(k)
            } else {
                None
            };
            if let Some(te) = time_end
                && s.get(te).is_some_and(|&u| is_js_whitespace(u))
                && s.len() > te + 1
            {
                return Some((&s[..te], &s[te + 1..]));
            }
        }
    }
    None
}

fn parse_ls_size_end(s: &[u16]) -> Option<usize> {
    // Either `\d+`, `\d+,\s*\d+`, or `[\d.]+[KMGTP]?`. Check with required trailing `\s+`.
    if let Some(comma) = s.iter().position(|&u| u == u16::from(b','))
        && comma > 0
        && s[..comma].iter().all(|&u| is_ascii_digit(u))
    {
        let mut k = comma + 1;
        while k < s.len() && is_js_whitespace(s[k]) {
            k += 1;
        }
        let d_start = k;
        while k < s.len() && is_ascii_digit(s[k]) {
            k += 1;
        }
        if k > d_start && s.get(k).is_some_and(|&u| is_js_whitespace(u)) {
            return Some(k);
        }
    }
    let mut k = 0;
    while k < s.len() && (is_ascii_digit(s[k]) || s[k] == u16::from(b'.')) {
        k += 1;
    }
    if k == 0 {
        return None;
    }
    if matches!(s.get(k), Some(0x004B | 0x004D | 0x0047 | 0x0054 | 0x0050)) {
        k += 1;
    }
    s.get(k).is_some_and(|&u| is_js_whitespace(u)).then_some(k)
}

fn parse_ls_row(line: &[u16]) -> Option<LsEntry<'_>> {
    if line.len() < 16 {
        return None;
    }
    let ftype = line[0];
    if !matches!(
        ftype,
        0x002D | 0x0064 | 0x006C | 0x0063 | 0x0062 | 0x0070 | 0x0073
    ) {
        return None;
    }
    let perm = &line[1..10];
    if !perm.iter().all(|&u| {
        matches!(
            u,
            0x0072 | 0x0077 | 0x0078 | 0x0073 | 0x0053 | 0x0074 | 0x0054 | 0x002D
        )
    }) {
        return None;
    }
    let mut k = 10;
    let mark = if matches!(line.get(k), Some(0x002E | 0x002B | 0x0040)) {
        k += 1;
        Some(line[10])
    } else {
        None
    };
    let mut ws = k;
    while ws < line.len() && is_js_whitespace(line[ws]) {
        ws += 1;
    }
    if ws == k {
        return None;
    }
    k = ws;
    // links \d+
    let links_start = k;
    while k < line.len() && is_ascii_digit(line[k]) {
        k += 1;
    }
    if k == links_start || !line.get(k).is_some_and(|&u| is_js_whitespace(u)) {
        return None;
    }
    while k < line.len() && is_js_whitespace(line[k]) {
        k += 1;
    }
    let owner_start = k;
    while k < line.len() && !is_js_whitespace(line[k]) {
        k += 1;
    }
    if k == owner_start || !line.get(k).is_some_and(|&u| is_js_whitespace(u)) {
        return None;
    }
    let owner = &line[owner_start..k];
    while k < line.len() && is_js_whitespace(line[k]) {
        k += 1;
    }
    let group_start = k;
    while k < line.len() && !is_js_whitespace(line[k]) {
        k += 1;
    }
    if k == group_start || !line.get(k).is_some_and(|&u| is_js_whitespace(u)) {
        return None;
    }
    let group = &line[group_start..k];
    while k < line.len() && is_js_whitespace(line[k]) {
        k += 1;
    }
    let size_rel = parse_ls_size_end(&line[k..])?;
    let size = &line[k..k + size_rel];
    k += size_rel;
    while k < line.len() && is_js_whitespace(line[k]) {
        k += 1;
    }
    let (date, name) = parse_ls_date(&line[k..])?;
    Some(LsEntry {
        ftype,
        perm,
        mark,
        owner,
        group,
        size,
        date,
        name,
    })
}

fn octal_perm(perm: &[u16]) -> Vec<u16> {
    let mut special = 0u8;
    let mut digits = [0u8; 3];
    for k in 0..3 {
        let t = &perm[k * 3..k * 3 + 3];
        let mut v = 0u8;
        if t[0] == u16::from(b'r') {
            v += 4;
        }
        if t[1] == u16::from(b'w') {
            v += 2;
        }
        if matches!(t[2], 0x0078 | 0x0073 | 0x0074) {
            v += 1;
        }
        if matches!(t[2], 0x0073 | 0x0053 | 0x0074 | 0x0054) {
            special += [4, 2, 1][k];
        }
        digits[k] = v;
    }
    let mut out = Vec::with_capacity(4);
    if special != 0 {
        out.push(u16::from(b'0' + special));
    }
    for d in digits {
        out.push(u16::from(b'0' + d));
    }
    out
}

fn collapse_ws_single_space(s: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    let mut in_ws = false;
    for &u in s {
        if is_js_whitespace(u) {
            if !in_ws {
                out.push(u16::from(b' '));
                in_ws = true;
            }
        } else {
            out.push(u);
            in_ws = false;
        }
    }
    out
}

fn compact_ls(lines: Vec<Vec<u16>>, keep_date: bool, me: Option<&[u16]>) -> Vec<Vec<u16>> {
    let parsed: Vec<Option<LsEntry<'_>>> = lines.iter().map(|l| parse_ls_row(l)).collect();
    let mut owners: Vec<(Vec<u16>, usize)> = Vec::new();
    for p in parsed.iter().flatten() {
        let mut og = p.owner.to_vec();
        og.push(u16::from(b':'));
        og.extend_from_slice(p.group);
        if let Some((_, count)) = owners.iter_mut().find(|(k, _)| k == &og) {
            *count += 1;
        } else {
            owners.push((og, 1));
        }
    }

    let main_owner: Option<Vec<u16>> = if owners.is_empty() {
        None
    } else {
        let mut sorted = owners.clone();
        sorted.sort_by_key(|a| std::cmp::Reverse(a.1));
        Some(
            sorted
                .into_iter()
                .next()
                .map(|(k, _)| k)
                .unwrap_or_default(),
        )
    };

    let mut out = Vec::with_capacity(lines.len() + 1);
    if let Some(ref main_og) = main_owner {
        let colon = main_og
            .iter()
            .position(|&u| u == u16::from(b':'))
            .unwrap_or(main_og.len());
        let owner_part = &main_og[..colon];
        let same_as_me = me.is_some_and(|m| m == owner_part);
        if owners.len() > 1 || !same_as_me {
            let mut h = Vec::new();
            push_ascii(&mut h, "(owner ");
            h.extend_from_slice(main_og);
            if owners.len() > 1 {
                push_ascii(&mut h, " unless shown");
            }
            h.push(u16::from(b')'));
            out.push(h);
        }
    }

    for (i, l) in lines.iter().enumerate() {
        let Some(ref p) = parsed[i] else {
            let is_total =
                starts_with_ascii(l, b"total ") && l.get(6).is_some_and(|&u| is_ascii_digit(u));
            if !is_total {
                out.push(l.clone());
            }
            continue;
        };
        if eq_ascii(p.name, b"..") {
            continue;
        }
        let mut og = p.owner.to_vec();
        og.push(u16::from(b':'));
        og.extend_from_slice(p.group);

        let mut row = Vec::with_capacity(32);
        if !matches!(p.ftype, 0x002D | 0x0064 | 0x006C) {
            row.push(p.ftype);
        }
        row.extend_from_slice(&octal_perm(p.perm));
        if let Some(m) = p.mark
            && m != u16::from(b'.')
        {
            row.push(m);
        }
        if p.ftype != u16::from(b'd') {
            row.push(u16::from(b' '));
            row.extend_from_slice(p.size);
        }
        if main_owner.as_ref().is_none_or(|m| m != &og) {
            row.push(u16::from(b' '));
            row.extend_from_slice(&og);
        }
        if keep_date {
            row.push(u16::from(b' '));
            row.extend_from_slice(&collapse_ws_single_space(p.date));
        }
        row.push(u16::from(b' '));
        row.extend_from_slice(p.name);
        if p.ftype == u16::from(b'd') {
            row.push(u16::from(b'/'));
        }
        out.push(row);
    }
    out
}

fn compact_wc(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let rows: Vec<Vec<&[u16]>> = lines
        .iter()
        .map(|l| {
            let t = trim(l);
            if t.is_empty() {
                vec![&[][..]]
            } else {
                split_ws(t)
            }
        })
        .collect();
    let paths: Vec<&[u16]> = rows
        .iter()
        .filter(|r| r.len() > 1 && r.last().is_some_and(|p| !eq_ascii(p, b"total")))
        .filter_map(|r| r.last().copied())
        .collect();

    let dir: &[u16] = if paths.len() > 1
        && let Some(last_slash) = paths[0].iter().rposition(|&u| u == u16::from(b'/'))
    {
        &paths[0][..=last_slash]
    } else {
        &[]
    };
    let shared = !dir.is_empty()
        && paths
            .iter()
            .all(|p| p.starts_with(dir) && !p[dir.len()..].contains(&u16::from(b'/')));

    let mut out = Vec::with_capacity(rows.len() + usize::from(shared));
    if shared {
        out.push(dir.to_vec());
    }
    for r in rows {
        let mut line = Vec::new();
        for (idx, part) in r.iter().enumerate() {
            if idx > 0 {
                line.push(u16::from(b' '));
            }
            if shared && idx + 1 == r.len() && r.len() > 1 && !eq_ascii(part, b"total") {
                line.extend_from_slice(&part[dir.len()..]);
            } else {
                line.extend_from_slice(part);
            }
        }
        out.push(line);
    }
    out
}

fn parse_grep_match(l: &[u16]) -> Option<(&[u16], &[u16], &[u16])> {
    // Non-greedy `^(.+?):(\d+):(.*)$`
    for pos in 1..l.len() {
        if l[pos] != u16::from(b':') {
            continue;
        }
        let rest = &l[pos + 1..];
        let digits = rest.iter().take_while(|&&u| is_ascii_digit(u)).count();
        if digits > 0 && rest.get(digits) == Some(&u16::from(b':')) {
            return Some((&l[..pos], &rest[..digits], &rest[digits + 1..]));
        }
    }
    None
}

fn group_grep(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut cur: Option<Vec<u16>> = None;

    for l in lines {
        if let Some((file, line_no, text)) = parse_grep_match(&l) {
            if cur.as_deref() != Some(file) {
                cur = Some(file.to_vec());
                out.push(file.to_vec());
            }
            let mut row = Vec::with_capacity(line_no.len() + 1 + text.len());
            row.extend_from_slice(line_no);
            row.push(u16::from(b':'));
            row.extend_from_slice(text);
            out.push(row);
            continue;
        }
        if let Some(ref c) = cur
            && l.len() > c.len() + 2
            && l.starts_with(c)
            && l[c.len()] == u16::from(b'-')
        {
            let rest = &l[c.len() + 1..];
            let digits = rest.iter().take_while(|&&u| is_ascii_digit(u)).count();
            if digits > 0 && rest.get(digits) == Some(&u16::from(b'-')) {
                out.push(rest.to_vec());
                continue;
            }
        }
        if !eq_ascii(&l, b"--") {
            cur = None;
        }
        out.push(l);
    }
    out
}

fn collapse_blank_and_trim_newlines(s: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    let mut nl_run = 0usize;
    for &u in s {
        if u == 0x000A {
            nl_run += 1;
            if nl_run <= 2 {
                out.push(u);
            }
        } else {
            nl_run = 0;
            out.push(u);
        }
    }
    trim_newlines(&out).to_vec()
}

fn trim_newlines(s: &[u16]) -> &[u16] {
    let start = s.iter().position(|&u| u != 0x000A).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&u| u != 0x000A)
        .map_or(start, |i| i + 1);
    &s[start..end]
}

#[must_use]
pub fn filter_command(command: &[u16], text: &[u16], me: Option<&[u16]>) -> Option<Vec<u16>> {
    if text.is_empty() {
        return None;
    }
    let kinds = kinds_for(command);
    if kinds == 0 {
        return None;
    }
    let stripped = strip_quotes(command);
    let segs: Vec<Classified> = split_segments(&stripped)
        .into_iter()
        .filter_map(|s| {
            let t = trim(s);
            (!t.is_empty()).then(|| classify(t)).flatten()
        })
        .collect();
    let has = |flag: &[u8]| segs.iter().any(|c| c.has_flag(flag));

    let clean = strip_ansi(text);
    let mut lines: Vec<Vec<u16>> = split_lf(&clean)
        .into_iter()
        .map(|l| {
            let t = l.strip_suffix(&[0x000D]).unwrap_or(l);
            if t.contains(&0x000D) {
                t.split(|&u| u == 0x000D)
                    .rfind(|p| !p.is_empty())
                    .unwrap_or(&[])
                    .to_vec()
            } else {
                t.to_vec()
            }
        })
        .collect();

    if kinds & KIND_TEST != 0 {
        lines = go_verbose(drop_tap_passes(lines));
    }
    lines.retain(|l| !should_drop_line(kinds, l));
    if kinds & KIND_TEST != 0 {
        lines = compact_cargo(lines);
    }
    if kinds & KIND_GIT_STATUS != 0 {
        lines = short_status(lines);
    }
    if kinds & KIND_GIT_DIFF != 0 {
        lines = compact_stat(compact_diff(lines));
    }
    if kinds & KIND_GIT_LOG != 0 {
        lines = compact_git_log(lines);
    }
    if kinds & KIND_GIT_BRANCH != 0 {
        lines = compact_branches(lines, has(b"r") || has(b"--remotes"));
    }
    if kinds & KIND_LS != 0 {
        lines = compact_ls(lines, has(b"t"), me);
    }
    if kinds == KIND_WC {
        lines = compact_wc(lines);
    }
    if kinds == KIND_GREP {
        lines = group_grep(lines);
    }

    let refs: Vec<&[u16]> = lines.iter().map(Vec::as_slice).collect();
    let joined = join_u16(&refs, 0x000A);
    let out = collapse_blank_and_trim_newlines(&joined);
    (out.len() < trim_newlines(text).len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        KIND_GIT_STATUS, KIND_ORDER, env_assign_end, filter_command, kinds_for, wrappable,
    };
    use crate::utf16::{str_from_u16_lossy, u16_from_str};

    #[test]
    fn basic_filter_checks() {
        let _ = KIND_ORDER;
        assert_eq!(env_assign_end(&u16_from_str("FOO=1 BAR=\"a\" cmd"), 0), 14);
        assert_eq!(kinds_for(&u16_from_str("git status")), KIND_GIT_STATUS);
        assert!(wrappable(&u16_from_str("cargo test")));
        assert!(!wrappable(&u16_from_str("git push")));
        let out = filter_command(
            &u16_from_str("git status"),
            &u16_from_str("On branch main\nChanges not staged for commit:\n\tmodified:   x.js\n"),
            None,
        )
        .unwrap();
        assert_eq!(str_from_u16_lossy(&out), "* main\n M x.js");
    }
}
