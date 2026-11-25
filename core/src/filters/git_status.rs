#![forbid(unsafe_code)]

//! Git status porcelain folding and branch header compression.

use super::is_ascii_digit;
use crate::utf16::{
    eq_ascii, is_js_whitespace, is_line_terminator, join_u16, push_ascii, split_lf,
    starts_with_ascii, trim,
};

fn map_status_section(l: &[u16]) -> Option<u8> {
    const MAP: &[(&[u8], u8)] = &[
        (b"Changes to be committed:", b'x'),
        (b"Changes not staged for commit:", b'y'),
        (b"Unmerged paths:", b'u'),
        (b"Untracked files:", b'?'),
        (b"Ignored files:", b'!'),
    ];
    MAP.iter()
        .find(|&&(p, _)| eq_ascii(l, p))
        .map(|&(_, code)| code)
}

fn map_status_code(verb: &[u16], unmerged: bool) -> Option<&'static [u8]> {
    const UNMERGED: &[(&[u8], &[u8])] = &[
        (b"both modified", b"UU"),
        (b"added by us", b"AU"),
        (b"added by them", b"UA"),
        (b"deleted by us", b"DU"),
        (b"deleted by them", b"UD"),
        (b"both added", b"AA"),
        (b"both deleted", b"DD"),
    ];
    const STAGED: &[(&[u8], &[u8])] = &[
        (b"modified", b"M"),
        (b"new file", b"A"),
        (b"deleted", b"D"),
        (b"renamed", b"R"),
        (b"copied", b"C"),
        (b"typechange", b"T"),
    ];
    let table = if unmerged { UNMERGED } else { STAGED };
    table
        .iter()
        .find(|&&(v, _)| eq_ascii(verb, v))
        .map(|&(_, code)| code)
}

fn upsert_ordered(map: &mut Vec<(Vec<u16>, Vec<u16>)>, key: Vec<u16>, val: Vec<u16>) {
    if let Some((_, existing)) = map.iter_mut().find(|(k, _)| k == &key) {
        *existing = val;
    } else {
        map.push((key, val));
    }
}

fn record_status_tab_entry(sec: u8, e: &[u16], files: &mut Vec<(Vec<u16>, Vec<u16>)>) -> bool {
    if sec == b'?' || sec == b'!' {
        let prefix = if sec == b'?' { "??" } else { "!!" };
        let mut key = Vec::with_capacity(2 + e.len());
        push_ascii(&mut key, prefix);
        key.extend_from_slice(e);
        let mut val = Vec::with_capacity(3 + e.len());
        push_ascii(&mut val, prefix);
        val.push(u16::from(b' '));
        val.extend_from_slice(e);
        upsert_ordered(files, key, val);
        return true;
    }
    let Some(colon) = e.iter().position(|&u| u == u16::from(b':')) else {
        return false;
    };
    if colon == 0
        || !e[..colon]
            .iter()
            .all(|&u| matches!(u, 0x0061..=0x007A | 0x0020))
        || !e.get(colon + 1).is_some_and(|&u| is_js_whitespace(u))
    {
        return false;
    }
    let mut rest_idx = colon + 1;
    while rest_idx < e.len() && is_js_whitespace(e[rest_idx]) {
        rest_idx += 1;
    }
    let file = &e[rest_idx..];
    let Some(code) = map_status_code(&e[..colon], sec == b'u') else {
        return false;
    };
    let val = format_status_row(sec, code, file, files);
    upsert_ordered(files, file.to_vec(), val);
    true
}

fn format_status_row(
    sec: u8,
    code: &[u8],
    file: &[u16],
    files: &[(Vec<u16>, Vec<u16>)],
) -> Vec<u16> {
    let mut val = Vec::with_capacity(3 + file.len());
    if sec == b'u' {
        val.extend(code.iter().map(|&b| u16::from(b)));
    } else {
        let prev = files
            .iter()
            .find(|(k, _)| k.as_slice() == file)
            .map(|(_, v)| v.as_slice());
        let x = if sec == b'x' {
            u16::from(code[0])
        } else {
            prev.and_then(|p| p.first().copied())
                .unwrap_or(u16::from(b' '))
        };
        let y = if sec == b'y' {
            u16::from(code[0])
        } else {
            u16::from(b' ')
        };
        val.push(x);
        val.push(y);
    }
    val.push(u16::from(b' '));
    val.extend_from_slice(file);
    val
}

pub(crate) fn short_status(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::with_capacity(lines.len());
    let mut files: Vec<(Vec<u16>, Vec<u16>)> = Vec::new();
    let mut sec: Option<u8> = None;
    let mut at: Option<usize> = None;

    for l in lines {
        if let Some(s) = map_status_section(&l) {
            sec = Some(s);
            at.get_or_insert(out.len());
        } else if trim(&l).is_empty() {
            sec = None;
        } else if sec.is_some_and(|s| {
            l.first() == Some(&0x0009) && record_status_tab_entry(s, &l[1..], &mut files)
        }) {
            continue;
        } else if !(starts_with_ascii(&l, b"no changes added to commit")
            || starts_with_ascii(&l, b"nothing added to commit but untracked files present"))
        {
            out.push(l);
        }
    }
    if let Some(pos) = at {
        out.splice(pos..pos, files.into_iter().map(|(_, v)| v));
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

fn line_bounds(text: &[u16], pos: usize, term_only_lf: bool) -> usize {
    text[pos..]
        .iter()
        .position(|&u| {
            if term_only_lf {
                u == 0x000A
            } else {
                is_line_terminator(u)
            }
        })
        .map_or(text.len(), |x| pos + x)
}

fn match_on_branch_l1(line: &[u16]) -> Option<&[u16]> {
    starts_with_ascii(line, b"On branch ")
        .then(|| &line[10..])
        .filter(|b| !b.is_empty() && !b.iter().any(|&u| is_js_whitespace(u)))
}

fn splice_branch_replacement(text: &[u16], start: usize, end: usize, mid: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(start + mid.len() + text.len() - end);
    out.extend_from_slice(&text[..start]);
    out.extend_from_slice(mid);
    out.extend_from_slice(&text[end..]);
    out
}

fn rewrite_branch_header_up_to_date(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let line_end = line_bounds(&text, pos, true);
        if let Some(branch) = match_on_branch_l1(&text[pos..line_end])
            && line_end < text.len()
        {
            let l2_end = line_bounds(&text, line_end + 1, false);
            let l2 = &text[line_end + 1..l2_end];
            if starts_with_ascii(l2, b"Your branch is up to date with '")
                && let Some((up, rest)) = parse_until_single_quote(&l2[32..])
                && !up.is_empty()
                && eq_ascii(rest, b".")
            {
                let mut hdr = Vec::new();
                push_ascii(&mut hdr, "* ");
                hdr.extend_from_slice(branch);
                push_ascii(&mut hdr, "...");
                hdr.extend_from_slice(up);
                return splice_branch_replacement(&text, pos, l2_end, &hdr);
            }
        }
        pos = if line_end == text.len() {
            text.len()
        } else {
            line_end + 1
        };
    }
    text
}

fn parse_ahead_behind_l2(l2: &[u16]) -> Option<(&'static str, &[u16], &[u16])> {
    let (dir, after_quote) = starts_with_ascii(l2, b"Your branch is ahead of '")
        .then(|| ("ahead", &l2[25..]))
        .or_else(|| {
            starts_with_ascii(l2, b"Your branch is behind '").then(|| ("behind", &l2[23..]))
        })?;
    let (up, rest) = parse_until_single_quote(after_quote)?;
    if up.is_empty() || !starts_with_ascii(rest, b" by ") {
        return None;
    }
    let after_by = &rest[4..];
    let digits = after_by.iter().take_while(|&&u| is_ascii_digit(u)).count();
    let tail = &after_by[digits..];
    let ok = digits > 0
        && (eq_ascii(tail, b" commit.")
            || eq_ascii(tail, b" commits.")
            || eq_ascii(tail, b" commit, and can be fast-forwarded.")
            || eq_ascii(tail, b" commits, and can be fast-forwarded."));
    ok.then_some((dir, up, &after_by[..digits]))
}

fn rewrite_branch_header_ahead_behind(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let line_end = line_bounds(&text, pos, true);
        if let Some(branch) = match_on_branch_l1(&text[pos..line_end])
            && line_end < text.len()
        {
            let l2_end = line_bounds(&text, line_end + 1, false);
            if let Some((dir, up, count)) = parse_ahead_behind_l2(&text[line_end + 1..l2_end]) {
                let mut hdr = Vec::new();
                push_ascii(&mut hdr, "* ");
                hdr.extend_from_slice(branch);
                push_ascii(&mut hdr, "...");
                hdr.extend_from_slice(up);
                push_ascii(&mut hdr, " [");
                push_ascii(&mut hdr, dir);
                hdr.push(u16::from(b' '));
                hdr.extend_from_slice(count);
                hdr.push(u16::from(b']'));
                return splice_branch_replacement(&text, pos, l2_end, &hdr);
            }
        }
        pos = if line_end == text.len() {
            text.len()
        } else {
            line_end + 1
        };
    }
    text
}

fn parse_diverged_l2_l3<'a>(
    l2: &'a [u16],
    l3: &'a [u16],
) -> Option<(&'a [u16], &'a [u16], &'a [u16])> {
    if !starts_with_ascii(l2, b"Your branch and '") {
        return None;
    }
    let (up, rest) = parse_until_single_quote(&l2[17..])?;
    if up.is_empty() || !eq_ascii(rest, b" have diverged,") || !starts_with_ascii(l3, b"and have ")
    {
        return None;
    }
    let after_have = &l3[9..];
    let d1 = after_have
        .iter()
        .take_while(|&&u| is_ascii_digit(u))
        .count();
    if d1 == 0 || !starts_with_ascii(&after_have[d1..], b" and ") {
        return None;
    }
    let after_and = &after_have[d1 + 5..];
    let d2 = after_and.iter().take_while(|&&u| is_ascii_digit(u)).count();
    (d2 > 0 && eq_ascii(&after_and[d2..], b" different commits each, respectively.")).then_some((
        up,
        &after_have[..d1],
        &after_and[..d2],
    ))
}

fn rewrite_branch_header_diverged(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let l1_end = line_bounds(&text, pos, true);
        if let Some(branch) = match_on_branch_l1(&text[pos..l1_end])
            && l1_end < text.len()
        {
            let l2_end = line_bounds(&text, l1_end + 1, true);
            if l2_end < text.len() {
                let l3_end = line_bounds(&text, l2_end + 1, false);
                if let Some((up, c1, c2)) =
                    parse_diverged_l2_l3(&text[l1_end + 1..l2_end], &text[l2_end + 1..l3_end])
                {
                    let mut hdr = Vec::new();
                    push_ascii(&mut hdr, "* ");
                    hdr.extend_from_slice(branch);
                    push_ascii(&mut hdr, "...");
                    hdr.extend_from_slice(up);
                    push_ascii(&mut hdr, " [ahead ");
                    hdr.extend_from_slice(c1);
                    push_ascii(&mut hdr, ", behind ");
                    hdr.extend_from_slice(c2);
                    hdr.push(u16::from(b']'));
                    return splice_branch_replacement(&text, pos, l3_end, &hdr);
                }
            }
        }
        pos = if l1_end == text.len() {
            text.len()
        } else {
            l1_end + 1
        };
    }
    text
}

fn rewrite_branch_header_plain(text: Vec<u16>) -> Vec<u16> {
    let mut pos = 0;
    while pos < text.len() {
        let end = line_bounds(&text, pos, false);
        if let Some(branch) = match_on_branch_l1(&text[pos..end]) {
            let mut hdr = Vec::new();
            push_ascii(&mut hdr, "* ");
            hdr.extend_from_slice(branch);
            return splice_branch_replacement(&text, pos, end, &hdr);
        }
        pos = if end == text.len() {
            text.len()
        } else {
            end + 1
        };
    }
    text
}
