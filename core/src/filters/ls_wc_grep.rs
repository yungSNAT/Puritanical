#![forbid(unsafe_code)]

//! Compaction for `ls -l`, `wc`, `grep`/`rg`, plus trailing blank-line cleanup.

use super::is_ascii_digit;
use super::shell::split_ws;
use crate::utf16::{eq_ascii, is_js_whitespace, is_js_word, push_ascii, starts_with_ascii, trim};

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

fn parse_ls_iso_hms(s: &[u16]) -> Option<(&[u16], &[u16])> {
    let mut k = 11;
    while k < s.len() && (is_ascii_digit(s[k]) || matches!(s[k], 0x003A | 0x002E)) {
        k += 1;
    }
    if s.get(k) == Some(&u16::from(b' '))
        && s.len() > k + 7
        && matches!(s[k + 1], 0x002B | 0x002D)
        && s[k + 2..k + 6].iter().all(|&u| is_ascii_digit(u))
        && is_js_whitespace(s[k + 6])
    {
        return Some((&s[..k + 6], &s[k + 7..]));
    }
    (s.get(k).is_some_and(|&u| is_js_whitespace(u)) && s.len() > k + 1)
        .then(|| (&s[..k], &s[k + 1..]))
}

fn parse_ls_iso_date(s: &[u16]) -> Option<(&[u16], &[u16])> {
    let valid_ymd = s.len() >= 10
        && s[..4].iter().all(|&u| is_ascii_digit(u))
        && s[4] == u16::from(b'-')
        && s[5..7].iter().all(|&u| is_ascii_digit(u))
        && s[7] == u16::from(b'-')
        && s[8..10].iter().all(|&u| is_ascii_digit(u));
    if !valid_ymd {
        return None;
    }
    if s.get(10) == Some(&u16::from(b' '))
        && s.get(11)
            .is_some_and(|&u| is_ascii_digit(u) || matches!(u, 0x003A | 0x002E))
        && let Some(hms) = parse_ls_iso_hms(s)
    {
        return Some(hms);
    }
    (s.get(10).is_some_and(|&u| is_js_whitespace(u)) && s.len() > 11).then(|| (&s[..10], &s[11..]))
}

fn parse_ls_time_or_year_end(s: &[u16], mut k: usize) -> Option<usize> {
    let t_start = k;
    while k < s.len() && is_ascii_digit(s[k]) && k - t_start < 4 {
        k += 1;
    }
    let dlen = k - t_start;
    if (1..=2).contains(&dlen)
        && s.get(k) == Some(&u16::from(b':'))
        && s.get(k + 1..k + 3)
            .is_some_and(|m| m.iter().all(|&u| is_ascii_digit(u)))
    {
        return Some(k + 3);
    }
    (dlen == 4).then_some(k)
}

fn parse_ls_short_date(s: &[u16]) -> Option<(&[u16], &[u16])> {
    if !(s.len() >= 12 && s[..3].iter().all(|&u| is_js_word(u)) && is_js_whitespace(s[3])) {
        return None;
    }
    let mut k = skip_ws(s, 3);
    let d_start = k;
    while k < s.len() && is_ascii_digit(s[k]) && k - d_start < 2 {
        k += 1;
    }
    if k == d_start || !s.get(k).is_some_and(|&u| is_js_whitespace(u)) {
        return None;
    }
    k = skip_ws(s, k);
    let time_end = parse_ls_time_or_year_end(s, k)?;
    (s.get(time_end).is_some_and(|&u| is_js_whitespace(u)) && s.len() > time_end + 1)
        .then(|| (&s[..time_end], &s[time_end + 1..]))
}

fn parse_ls_date(s: &[u16]) -> Option<(&[u16], &[u16])> {
    parse_ls_iso_date(s).or_else(|| parse_ls_short_date(s))
}

fn parse_ls_size_end(s: &[u16]) -> Option<usize> {
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
    k += usize::from(matches!(
        s.get(k),
        Some(0x004B | 0x004D | 0x0047 | 0x0054 | 0x0050)
    ));
    s.get(k).is_some_and(|&u| is_js_whitespace(u)).then_some(k)
}

fn skip_ws(s: &[u16], mut k: usize) -> usize {
    while k < s.len() && is_js_whitespace(s[k]) {
        k += 1;
    }
    k
}

fn take_non_ws_field(s: &[u16], mut k: usize) -> Option<(&[u16], usize)> {
    k = skip_ws(s, k);
    let start = k;
    while k < s.len() && !is_js_whitespace(s[k]) {
        k += 1;
    }
    (k > start && s.get(k).is_some_and(|&u| is_js_whitespace(u))).then(|| (&s[start..k], k))
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
    let mark = matches!(line.get(10), Some(0x002E | 0x002B | 0x0040)).then(|| line[10]);
    let after_mark = 10 + usize::from(mark.is_some());
    let k = skip_ws(line, after_mark);
    if k == after_mark {
        return None;
    }
    let (links, k) = take_non_ws_field(line, k)?;
    if !links.iter().all(|&u| is_ascii_digit(u)) {
        return None;
    }
    let (owner, k) = take_non_ws_field(line, k)?;
    let (group, k) = take_non_ws_field(line, k)?;
    let k = skip_ws(line, k);
    let size_rel = parse_ls_size_end(&line[k..])?;
    let size = &line[k..k + size_rel];
    let (date, name) = parse_ls_date(&line[skip_ws(line, k + size_rel)..])?;
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
        let v = u8::from(t[0] == u16::from(b'r')) * 4
            + u8::from(t[1] == u16::from(b'w')) * 2
            + u8::from(matches!(t[2], 0x0078 | 0x0073 | 0x0074));
        if matches!(t[2], 0x0073 | 0x0053 | 0x0074 | 0x0054) {
            special += [4, 2, 1][k];
        }
        digits[k] = v;
    }
    let mut out = Vec::with_capacity(4);
    if special != 0 {
        out.push(u16::from(b'0' + special));
    }
    out.extend(digits.into_iter().map(|d| u16::from(b'0' + d)));
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

fn compute_main_owner(parsed: &[Option<LsEntry<'_>>]) -> (Option<Vec<u16>>, usize) {
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
    let n_owners = owners.len();
    owners.sort_by_key(|a| std::cmp::Reverse(a.1));
    (owners.into_iter().next().map(|(k, _)| k), n_owners)
}

fn format_ls_entry(p: &LsEntry<'_>, main_owner: Option<&[u16]>, keep_date: bool) -> Vec<u16> {
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
    if main_owner.is_none_or(|m| m != og.as_slice()) {
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
    row
}

pub(crate) fn compact_ls(
    lines: Vec<Vec<u16>>,
    keep_date: bool,
    me: Option<&[u16]>,
) -> Vec<Vec<u16>> {
    let parsed: Vec<Option<LsEntry<'_>>> = lines.iter().map(|l| parse_ls_row(l)).collect();
    let (main_owner, n_owners) = compute_main_owner(&parsed);
    let mut out = Vec::with_capacity(lines.len() + 1);
    if let Some(ref main_og) = main_owner {
        let colon = main_og
            .iter()
            .position(|&u| u == u16::from(b':'))
            .unwrap_or(main_og.len());
        if n_owners > 1 || me != Some(&main_og[..colon]) {
            let mut h = Vec::new();
            push_ascii(&mut h, "(owner ");
            h.extend_from_slice(main_og);
            if n_owners > 1 {
                push_ascii(&mut h, " unless shown");
            }
            h.push(u16::from(b')'));
            out.push(h);
        }
    }
    for (i, l) in lines.iter().enumerate() {
        let Some(ref p) = parsed[i] else {
            if !(starts_with_ascii(l, b"total ") && l.get(6).is_some_and(|&u| is_ascii_digit(u))) {
                out.push(l.clone());
            }
            continue;
        };
        if !eq_ascii(p.name, b"..") {
            out.push(format_ls_entry(p, main_owner.as_deref(), keep_date));
        }
    }
    out
}

fn shared_wc_dir<'a>(rows: &[Vec<&'a [u16]>]) -> &'a [u16] {
    let paths: Vec<&[u16]> = rows
        .iter()
        .filter(|r| r.len() > 1 && r.last().is_some_and(|p| !eq_ascii(p, b"total")))
        .filter_map(|r| r.last().copied())
        .collect();
    let Some(dir) = paths
        .first()
        .filter(|_| paths.len() > 1)
        .and_then(|p0| {
            p0.iter()
                .rposition(|&u| u == u16::from(b'/'))
                .map(|s| &p0[..=s])
        })
        .filter(|d| !d.is_empty())
    else {
        return &[];
    };
    let all_in_dir = paths
        .iter()
        .all(|p| p.starts_with(dir) && !p[dir.len()..].contains(&u16::from(b'/')));
    if all_in_dir { dir } else { &[] }
}

pub(crate) fn compact_wc(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let rows: Vec<Vec<&[u16]>> = lines
        .iter()
        .map(|l| match trim(l) {
            [] => vec![&[][..]],
            t => split_ws(t),
        })
        .collect();
    let dir = shared_wc_dir(&rows);
    let shared = !dir.is_empty();
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
            let tail = if shared && idx + 1 == r.len() && r.len() > 1 && !eq_ascii(part, b"total") {
                &part[dir.len()..]
            } else {
                part
            };
            line.extend_from_slice(tail);
        }
        out.push(line);
    }
    out
}

fn parse_grep_match(l: &[u16]) -> Option<(&[u16], &[u16], &[u16])> {
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

pub(crate) fn group_grep(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
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

pub(crate) fn collapse_blank_and_trim_newlines(s: &[u16]) -> Vec<u16> {
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

pub(crate) fn trim_newlines(s: &[u16]) -> &[u16] {
    let start = s.iter().position(|&u| u != 0x000A).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&u| u != 0x000A)
        .map_or(start, |i| i + 1);
    &s[start..end]
}
