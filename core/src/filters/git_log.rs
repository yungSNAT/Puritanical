#![forbid(unsafe_code)]

//! Git log compaction: collapsed headers, ISO dates and capped commit bodies.

use super::{is_ascii_digit, is_ascii_hex_lower};
use crate::utf16::{
    eq_ascii, is_js_whitespace, is_js_word, push_ascii, push_usize, starts_with_ascii, trim,
};

const MONTHS: &[(&[u8], &str)] = &[
    (b"Jan", "01"),
    (b"Feb", "02"),
    (b"Mar", "03"),
    (b"Apr", "04"),
    (b"May", "05"),
    (b"Jun", "06"),
    (b"Jul", "07"),
    (b"Aug", "08"),
    (b"Sep", "09"),
    (b"Oct", "10"),
    (b"Nov", "11"),
    (b"Dec", "12"),
];

fn is_commit_header(l: &[u16]) -> Option<(&[u16], &[u16])> {
    let rest = starts_with_ascii(l, b"commit ").then(|| &l[7..])?;
    (rest.len() >= 40 && rest[..40].iter().all(|&u| is_ascii_hex_lower(u)))
        .then(|| (&rest[..40], &rest[40..]))
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

fn month_num(m: &[u8]) -> Option<&'static str> {
    let _ = m;
    None
}

fn month_num_u16(m: &[u16]) -> Option<&'static str> {
    let _ = month_num;
    MONTHS
        .iter()
        .find(|&&(k, _)| eq_ascii(m, k))
        .map(|&(_, v)| v)
}

fn parse_year_after_hms(raw: &[u16], mut k: usize) -> Option<&[u16]> {
    let t_start = k;
    while k < raw.len() && (is_ascii_digit(raw[k]) || raw[k] == u16::from(b':')) {
        k += 1;
    }
    if k == t_start || raw.get(k) != Some(&u16::from(b' ')) {
        return None;
    }
    k += 1;
    let valid = raw.len() == k + 10
        && raw[k..k + 4].iter().all(|&u| is_ascii_digit(u))
        && raw[k + 4] == u16::from(b' ')
        && matches!(raw[k + 5], 0x002B | 0x002D)
        && raw[k + 6..k + 10].iter().all(|&u| is_ascii_digit(u));
    valid.then_some(&raw[k..k + 4])
}

fn short_date(raw: &[u16]) -> Vec<u16> {
    if raw.len() >= 20
        && raw[..3].iter().all(|&u| is_js_word(u))
        && raw[3] == u16::from(b' ')
        && raw[4..7].iter().all(|&u| is_js_word(u))
        && raw[7] == u16::from(b' ')
        && let Some(mon) = month_num_u16(&raw[4..7])
    {
        let mut k = 8;
        while k < raw.len() && is_ascii_digit(raw[k]) && k - 8 < 2 {
            k += 1;
        }
        let day = &raw[8..k];
        if !day.is_empty()
            && raw.get(k) == Some(&u16::from(b' '))
            && let Some(year) = parse_year_after_hms(raw, k + 1)
        {
            let mut out = year.to_vec();
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
    raw.to_vec()
}

fn collect_unique_authors(lines: &[Vec<u16>]) -> Vec<Vec<u16>> {
    let mut authors: Vec<Vec<u16>> = Vec::new();
    for l in lines {
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
    authors
}

fn parse_commit_meta_line(line: &[u16]) -> Option<(u8, &[u16])> {
    let (kind, after_colon) = if starts_with_ascii(line, b"Merge:") {
        (0u8, &line[6..])
    } else if starts_with_ascii(line, b"Author:") {
        (1u8, &line[7..])
    } else if starts_with_ascii(line, b"Date:") {
        (2u8, &line[5..])
    } else {
        return None;
    };
    let k = after_colon
        .iter()
        .take_while(|&&u| is_js_whitespace(u))
        .count();
    (k > 0).then_some((kind, &after_colon[k..]))
}

fn parse_commit_meta(
    lines: &[Vec<u16>],
    start: usize,
    one_author: Option<&[u16]>,
) -> (Vec<u16>, Vec<u16>, usize) {
    let mut meta = Vec::new();
    let mut merge = Vec::new();
    let mut j = start;
    while let Some((kind, val)) = lines.get(j).and_then(|l| parse_commit_meta_line(l)) {
        match kind {
            0 => {
                merge.clear();
                push_ascii(&mut merge, " merge ");
                merge.extend_from_slice(val);
            }
            1 if one_author.is_none() => {
                meta.push(u16::from(b' '));
                meta.extend_from_slice(strip_author_email(val));
            }
            2 => {
                let mut prefix = vec![u16::from(b' ')];
                prefix.extend_from_slice(&short_date(val));
                prefix.extend_from_slice(&meta);
                meta = prefix;
            }
            _ => {}
        }
        j += 1;
    }
    (meta, merge, j)
}

fn collect_commit_body(lines: &[Vec<u16>], meta_end: usize) -> (Vec<&[u16]>, usize) {
    let mut b_idx = meta_end + usize::from(lines.get(meta_end).is_some_and(Vec::is_empty));
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
    (body, b_idx)
}

fn emit_commit_block(
    hash: &[u16],
    rest_hdr: &[u16],
    meta: &[u16],
    merge: &[u16],
    body: &[&[u16]],
    many: bool,
    out: &mut Vec<Vec<u16>>,
) {
    const BODY_CAP: usize = 3;
    let subject = body.first().copied().unwrap_or(&[]);
    let body_rest = if body.is_empty() { &[][..] } else { &body[1..] };

    let mut hdr = hash[..10].to_vec();
    hdr.extend_from_slice(rest_hdr);
    hdr.extend_from_slice(merge);
    hdr.push(u16::from(b' '));
    hdr.extend_from_slice(subject);
    push_ascii(&mut hdr, " (");
    hdr.extend_from_slice(trim(meta));
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
}

pub(crate) fn compact_git_log(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let many = lines
        .iter()
        .filter(|l| is_commit_header(l).is_some())
        .count()
        > 1;
    let authors = collect_unique_authors(&lines);
    let one_author = (many && authors.len() == 1).then(|| authors[0].as_slice());

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
        let (meta, merge, meta_end) = parse_commit_meta(&lines, i + 1, one_author);
        let (body, mut p) = collect_commit_body(&lines, meta_end);
        emit_commit_block(hash, rest_hdr, &meta, &merge, &body, many, &mut out);
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
