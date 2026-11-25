#![forbid(unsafe_code)]

//! Shell command classification, ordered `KIND_RULES` matching and `wrappable`.

use super::shell::{split_segments, split_ws, strip_quotes, strip_runners};
use super::{is_ascii_alpha, is_ascii_digit};
use crate::utf16::{
    contains_ascii_word, eq_ascii, eq_ascii_ci, is_js_whitespace, is_js_word, push_ascii,
    starts_with_ascii, trim,
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

const TEST_PROGRAMS: &[&[u8]] = &[
    b"pytest", b"py.test", b"jest", b"vitest", b"mocha", b"ava", b"tap", b"rspec", b"phpunit",
];
const PKG_MANAGERS: &[&[u8]] = &[b"npm", b"pnpm", b"yarn", b"bun"];

pub(crate) struct Classified {
    pub kinds: u16,
    pub ordered_kinds: Vec<&'static str>,
    pub prog: Vec<u16>,
    pub sub: Vec<u16>,
    pub flags: Vec<Vec<u16>>,
    pub text: Vec<u16>,
}

impl Classified {
    pub fn has_flag(&self, flag: &[u8]) -> bool {
        self.flags.iter().any(|f| eq_ascii(f, flag))
    }

    pub fn prog_eq(&self, name: &[u8]) -> bool {
        eq_ascii(&self.prog, name)
    }

    pub fn sub_in(&self, names: &[&[u8]]) -> bool {
        names.iter().any(|&n| eq_ascii(&self.sub, n))
    }
}

pub(crate) fn cmd_has_flag(segs: &[Classified], flag: &[u8]) -> bool {
    segs.iter().any(|c| c.has_flag(flag))
}

fn extract_prog(words: &[&[u16]]) -> Vec<u16> {
    let raw = words.first().copied().unwrap_or(&[]);
    let after_sep = match raw
        .iter()
        .rposition(|&u| u == u16::from(b'/') || u == u16::from(b'\\'))
    {
        Some(idx) => &raw[idx + 1..],
        None => raw,
    };
    let n = after_sep.len();
    if n >= 4
        && (eq_ascii_ci(&after_sep[n - 4..], b".exe") || eq_ascii_ci(&after_sep[n - 4..], b".cmd"))
    {
        after_sep[..n - 4].to_vec()
    } else {
        after_sep.to_vec()
    }
}

fn extract_subcommand_and_flags(words: &[&[u16]]) -> (Vec<u16>, Vec<Vec<u16>>) {
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

    let mut flags = Vec::new();
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
            flags.extend(w[1..].iter().map(|&ch| vec![ch]));
        }
    }
    (sub, flags)
}

pub(crate) fn classify(segment: &[u16]) -> Option<Classified> {
    let parts: Vec<&[u16]> = segment.split(|&u| u == u16::from(b'|')).map(trim).collect();
    for p in parts.iter().skip(1) {
        let first = p.split(|&u| is_js_whitespace(u)).next().unwrap_or(&[]);
        if !eq_ascii(first, b"head") && !eq_ascii(first, b"tail") {
            return None;
        }
    }
    let text = strip_runners(parts[0]);
    let words: Vec<&[u16]> = split_ws(&text)
        .into_iter()
        .filter(|w| {
            !w.is_empty()
                && !matches!(w[0], 0x003C | 0x003E | 0x0026)
                && !(is_ascii_digit(w[0]) && w.get(1) == Some(&0x003E))
        })
        .collect();
    let prog = extract_prog(&words);
    if eq_ascii(&prog, b"rtk") {
        return None;
    }
    let (sub, flags) = extract_subcommand_and_flags(&words);
    let mut c = Classified {
        kinds: 0,
        ordered_kinds: Vec::new(),
        prog,
        sub,
        flags,
        text,
    };
    compute_kinds(&mut c);
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
    [&b"--format"[..], b"--pretty", b"--oneline", b"--graph"]
        .iter()
        .any(|pat| {
            (0..text.len()).any(|i| {
                starts_with_ascii(&text[i..], pat)
                    && (i + pat.len() == text.len() || !is_js_word(text[i + pat.len()]))
            })
        })
}

fn is_pip_prog(prog: &[u16]) -> bool {
    if !starts_with_ascii(prog, b"pip") {
        return false;
    }
    let rest = &prog[3..];
    if rest.is_empty() || rest.iter().all(|&u| is_ascii_digit(u)) {
        return true;
    }
    rest.iter()
        .position(|&u| u == u16::from(b'.'))
        .is_some_and(|dot| {
            rest[..dot].iter().all(|&u| is_ascii_digit(u))
                && !rest[dot + 1..].is_empty()
                && rest[dot + 1..].iter().all(|&u| is_ascii_digit(u))
        })
}

fn is_pkg_run_test(c: &Classified) -> bool {
    if !PKG_MANAGERS.iter().any(|&p| c.prog_eq(p)) || !c.sub_in(&[b"run", b"run-script"]) {
        return false;
    }
    (1..c.text.len()).any(|i| {
        if !is_js_whitespace(c.text[i - 1]) {
            return false;
        }
        let r = &c.text[i..];
        let after = if starts_with_ascii(r, b"run-script") {
            10
        } else if starts_with_ascii(r, b"run") {
            3
        } else {
            0
        };
        if after == 0 || !r.get(after).is_some_and(|&u| is_js_whitespace(u)) {
            return false;
        }
        let mut k = after;
        while k < r.len() && is_js_whitespace(r[k]) {
            k += 1;
        }
        starts_with_ascii(&r[k..], b"test")
    })
}

fn is_test_cmd(c: &Classified) -> bool {
    if TEST_PROGRAMS.iter().any(|&p| c.prog_eq(p))
        || (c.prog_eq(b"node") && contains_ws_flag(&c.text, b"--test"))
        || is_pkg_run_test(c)
    {
        return true;
    }
    if c.prog_eq(b"go") || c.prog_eq(b"deno") || c.prog_eq(b"dotnet") {
        c.sub_in(&[b"test"])
    } else if PKG_MANAGERS.iter().any(|&p| c.prog_eq(p)) {
        c.sub_in(&[b"test", b"t"])
    } else if c.prog_eq(b"cargo") {
        c.sub_in(&[b"test", b"nextest"])
    } else if c.prog_eq(b"make") {
        contains_ascii_word(&c.sub, b"test") || contains_ascii_word(&c.sub, b"check")
    } else {
        false
    }
}

fn is_progress_cmd(c: &Classified) -> bool {
    if c.prog_eq(b"docker-compose") || is_pip_prog(&c.prog) {
        return true;
    }
    if PKG_MANAGERS.iter().any(|&p| c.prog_eq(p)) {
        c.sub_in(&[b"install", b"i", b"ci", b"add", b"update", b"upgrade"])
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
    }
}

fn is_grep_cmd(c: &Classified) -> bool {
    c.prog_eq(b"rg")
        || ((c.prog_eq(b"grep") || c.prog_eq(b"egrep"))
            && (c.has_flag(b"n") || c.has_flag(b"--line-number"))
            && (c.has_flag(b"r")
                || c.has_flag(b"R")
                || c.has_flag(b"H")
                || c.has_flag(b"--recursive")))
}

fn push_kind(c: &mut Classified, bit: u16, name: &'static str) {
    c.kinds |= bit;
    if !c.ordered_kinds.contains(&name) {
        c.ordered_kinds.push(name);
    }
}

fn compute_git_kinds(c: &mut Classified) {
    if !c.prog_eq(b"git") {
        return;
    }
    if c.sub_in(&[b"status"])
        && !c.has_flag(b"s")
        && !c.has_flag(b"--short")
        && !c.has_flag(b"--porcelain")
    {
        push_kind(c, KIND_GIT_STATUS, "git-status");
    }
    if c.sub_in(&[b"push", b"pull", b"fetch", b"clone"]) {
        push_kind(c, KIND_PROGRESS, "progress");
    }
    if c.sub_in(&[b"log", b"show"]) && !has_git_log_skip_flag(&c.text) {
        push_kind(c, KIND_GIT_LOG, "git-log");
    }
    if c.sub_in(&[b"diff", b"show", b"log"]) {
        push_kind(c, KIND_GIT_DIFF, "git-diff");
    }
    if c.sub_in(&[b"branch"]) && !c.has_flag(b"v") && !c.has_flag(b"--verbose") {
        push_kind(c, KIND_GIT_BRANCH, "git-branch");
    }
}

/// Evaluates rules in exact JS `KIND_RULES` order so `Set` insertion order matches verbatim.
fn compute_kinds(c: &mut Classified) {
    compute_git_kinds(c);
    if c.prog_eq(b"ls") && c.has_flag(b"l") {
        push_kind(c, KIND_LS, "ls");
    }
    if c.prog_eq(b"wc") {
        push_kind(c, KIND_WC, "wc");
    }
    if is_grep_cmd(c) {
        push_kind(c, KIND_GREP, "grep");
    }
    if is_test_cmd(c) {
        push_kind(c, KIND_TEST, "test");
    }
    if is_progress_cmd(c) {
        push_kind(c, KIND_PROGRESS, "progress");
    }
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
pub fn kinds_for_csv(command: &[u16]) -> Option<Vec<u16>> {
    if trim(command).is_empty() {
        return None;
    }
    let stripped = strip_quotes(command);
    let mut names: Vec<&'static str> = Vec::new();
    for seg in split_segments(&stripped) {
        let t = trim(seg);
        if !t.is_empty()
            && let Some(c) = classify(t)
        {
            for name in c.ordered_kinds {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push(u16::from(b','));
        }
        push_ascii(&mut out, name);
    }
    Some(out)
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
