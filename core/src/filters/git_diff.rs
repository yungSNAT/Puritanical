#![forbid(unsafe_code)]

//! Git diff header compaction and diffstat collapsing.

use super::{is_ascii_digit, is_ascii_hex_lower};
use crate::utf16::{eq_ascii, find_ascii, is_js_whitespace, push_ascii, starts_with_ascii};

fn step_diff_mode_or_rename(h: &[u16], path: &mut Vec<u16>, notes: &mut Vec<Vec<u16>>) -> bool {
    if starts_with_ascii_digits_after(h, b"new file mode ") {
        notes.push(b"new".iter().map(|&x| u16::from(x)).collect());
        return true;
    }
    if starts_with_ascii_digits_after(h, b"deleted file mode ") {
        notes.push(b"deleted".iter().map(|&x| u16::from(x)).collect());
        return true;
    }
    if let Some(m) = starts_with_digits_slice(h, b"old mode ") {
        let mut note = Vec::new();
        push_ascii(&mut note, "mode ");
        note.extend_from_slice(m);
        notes.push(note);
        return true;
    }
    if let Some(m) = starts_with_digits_slice(h, b"new mode ") {
        let mut note = Vec::new();
        push_ascii(&mut note, "-> ");
        note.extend_from_slice(m);
        notes.push(note);
        return true;
    }
    if let Some(from) = starts_with_ascii(h, b"rename from ")
        .then(|| &h[12..])
        .or_else(|| starts_with_ascii(h, b"copy from ").then(|| &h[10..]))
    {
        let mut renamed = Vec::with_capacity(from.len() + 4 + path.len());
        renamed.extend_from_slice(from);
        push_ascii(&mut renamed, " -> ");
        renamed.extend_from_slice(path);
        *path = renamed;
        return true;
    }
    false
}

fn step_diff_header_line(h: &[u16], path: &mut Vec<u16>, notes: &mut Vec<Vec<u16>>) -> bool {
    if is_diff_index_line(h)
        || starts_with_ascii(h, b"similarity index ")
        || starts_with_ascii(h, b"dissimilarity index ")
        || starts_with_ascii(h, b"rename to ")
        || starts_with_ascii(h, b"copy to ")
        || starts_with_ascii(h, b"--- a/")
        || eq_ascii(h, b"--- /dev/null")
        || eq_ascii(h, b"+++ /dev/null")
    {
        return true;
    }
    if step_diff_mode_or_rename(h, path, notes) {
        return true;
    }
    if let Some(new_p) = starts_with_ascii(h, b"+++ b/").then(|| &h[6..]) {
        if !new_p.is_empty() && find_ascii(path, b" -> ").is_none() {
            *path = new_p.to_vec();
        }
        return true;
    }
    false
}

pub(crate) fn compact_diff(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let Some((_a, b_path)) = parse_diff_git_header(&lines[i]) else {
            out.push(lines[i].clone());
            i += 1;
            continue;
        };
        let mut path = b_path.to_vec();
        let mut notes: Vec<Vec<u16>> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() && step_diff_header_line(&lines[j], &mut path, &mut notes) {
            j += 1;
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
    (0..=rest.len().saturating_sub(3))
        .rev()
        .find(|&k| starts_with_ascii(&rest[k..], b" b/"))
        .map(|k| (&rest[..k], &rest[k + 3..]))
}

fn is_diff_index_line(h: &[u16]) -> bool {
    let Some(rest) = starts_with_ascii(h, b"index ").then(|| &h[6..]) else {
        return false;
    };
    let left = rest.iter().take_while(|&&u| is_ascii_hex_lower(u)).count();
    left > 0
        && starts_with_ascii(&rest[left..], b"..")
        && rest.get(left + 2).is_some_and(|&u| is_ascii_hex_lower(u))
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

fn parse_stat_rhs(rhs: &[u16]) -> Option<&[u16]> {
    if starts_with_ascii(rhs, b"Bin ") {
        return Some(rhs);
    }
    let digits = rhs.iter().take_while(|&&u| is_ascii_digit(u)).count();
    if digits == 0 {
        return None;
    }
    let after = &rhs[digits..];
    let tail = after.strip_prefix(&[u16::from(b' ')]).unwrap_or(after);
    tail.iter()
        .all(|&u| u == u16::from(b'+') || u == u16::from(b'-'))
        .then_some(&rhs[..digits])
}

fn parse_stat_line(line: &[u16]) -> Option<(&[u16], &[u16])> {
    if line.first() != Some(&u16::from(b' ')) || line.get(1).is_none_or(|&u| is_js_whitespace(u)) {
        return None;
    }
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
        if ws2_end > ws1_end + 1
            && let Some(rhs) = parse_stat_rhs(&body[ws2_end..])
        {
            return Some((&body[..pos], rhs));
        }
    }
    None
}

type StatSummary<'a> = (&'a [u16], Option<&'a [u16]>, Option<&'a [u16]>);

fn parse_stat_noun_count<'a>(
    slice: &'a [u16],
    plural: &[u8],
    singular: &[u8],
) -> Option<(&'a [u16], &'a [u16])> {
    let d_len = slice.iter().take_while(|&&u| is_ascii_digit(u)).count();
    if d_len == 0 {
        return None;
    }
    let after_d = &slice[d_len..];
    let rest = starts_with_ascii(after_d, plural)
        .then(|| &after_d[plural.len()..])
        .or_else(|| starts_with_ascii(after_d, singular).then(|| &after_d[singular.len()..]))?;
    Some((&slice[..d_len], rest))
}

fn parse_stat_summary(line: &[u16]) -> Option<StatSummary<'_>> {
    let after_sp = line.strip_prefix(&[u16::from(b' ')])?;
    let (files, mut rest) = parse_stat_noun_count(after_sp, b" files changed", b" file changed")?;
    let mut ins = None;
    let mut del = None;
    if let Some(d_slice) = starts_with_ascii(rest, b", ").then(|| &rest[2..])
        && let Some((ins_n, after_ins)) =
            parse_stat_noun_count(d_slice, b" insertions(+)", b" insertion(+)")
    {
        ins = Some(ins_n);
        rest = after_ins;
    }
    if !rest.is_empty() {
        let d_slice = starts_with_ascii(rest, b", ").then(|| &rest[2..])?;
        let (del_n, after_del) = parse_stat_noun_count(d_slice, b" deletions(-)", b" deletion(-)")?;
        if !after_del.is_empty() {
            return None;
        }
        del = Some(del_n);
    }
    Some((files, ins, del))
}

pub(crate) fn compact_stat(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
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
