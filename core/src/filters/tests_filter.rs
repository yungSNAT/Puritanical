#![forbid(unsafe_code)]

//! Test-runner line drops, Go verbose pruning, TAP stripping and Cargo summary compaction.

use super::is_ascii_digit;
use crate::utf16::{
    eq_ascii, find_ascii, is_js_whitespace, is_js_word, starts_with_ascii, trim_end, trim_start,
};

fn ends_with_pct_bracket(s: &[u16]) -> Option<usize> {
    let inner = s
        .strip_suffix(&[u16::from(b']')])?
        .strip_suffix(&[u16::from(b'%')])?;
    let mut k = inner.len();
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
    (k > 0 && inner[k - 1] == u16::from(b'[')).then_some(k - 1)
}

fn has_pytest_node_pass(body: &[u16]) -> bool {
    let Some(cc) = find_ascii(body, b"::") else {
        return false;
    };
    cc > 0
        && !body[..cc].iter().any(|&u| is_js_whitespace(u))
        && body.get(cc + 2).is_some_and(|&u| !is_js_whitespace(u))
        && body.len() >= 7
        && starts_with_ascii(&body[body.len() - 6..], b"PASSED")
        && is_js_whitespace(body[body.len() - 7])
}

fn matches_pytest_drop(l: &[u16]) -> bool {
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
            let dots = mid
                .iter()
                .take_while(|&&u| matches!(u, 0x002E | 0x0073 | 0x0078))
                .count();
            if dots > 0 && mid[dots..].iter().all(|&u| is_js_whitespace(u)) {
                return true;
            }
        }
        let body = trim_end(before_pct);
        if body.len() < before_pct.len() && has_pytest_node_pass(body) {
            return true;
        }
    }
    has_pytest_node_pass(l)
}

fn matches_cargo_scaffold_drop(l: &[u16], ws: &[u16]) -> bool {
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

pub(crate) fn matches_test_drop(l: &[u16]) -> bool {
    if matches_pytest_drop(l) {
        return true;
    }
    let ws = trim_start(l);
    if !ws.is_empty()
        && matches!(ws[0], 0x2713 | 0x2714 | 0x221A | 0x25B6)
        && ws.get(1).is_some_and(|&u| is_js_whitespace(u))
    {
        return true;
    }
    if starts_with_ascii(ws, b"PASS") && ws.get(4).is_some_and(|&u| is_js_whitespace(u)) {
        let after = trim_start(&ws[4..]);
        if !after.is_empty() && !is_js_whitespace(after[0]) {
            return true;
        }
    }
    if starts_with_ascii(l, b"test ")
        && l[5..]
            .iter()
            .position(|&u| is_js_whitespace(u))
            .is_some_and(|sp| sp > 0 && eq_ascii(&l[5 + sp..], b" ... ok"))
    {
        return true;
    }
    starts_with_ascii(ws, b"--- PASS: ")
        || starts_with_ascii(ws, b"# Subtest: ")
        || matches_cargo_scaffold_drop(l, ws)
}

fn collect_go_keep_names(lines: &[Vec<u16>]) -> Vec<Vec<u16>> {
    let mut keep: Vec<Vec<u16>> = Vec::new();
    for l in lines {
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
    keep
}

fn parse_go_frame_header(l: &[u16]) -> Option<(bool, Vec<u16>)> {
    if !starts_with_ascii(l, b"=== ") {
        return None;
    }
    for (tag, pause) in [
        (&b"RUN"[..], false),
        (b"PAUSE", true),
        (b"CONT", false),
        (b"NAME", false),
    ] {
        if starts_with_ascii(&l[4..], tag) {
            let k = 4 + tag.len();
            if k < l.len() && is_js_whitespace(l[k]) {
                let name: Vec<u16> = trim_start(&l[k..])
                    .iter()
                    .copied()
                    .take_while(|&u| !is_js_whitespace(u))
                    .collect();
                if !name.is_empty() {
                    return Some((pause, name));
                }
            }
            break;
        }
    }
    None
}

pub(crate) fn go_verbose(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    if !lines
        .iter()
        .any(|l| starts_with_ascii(l, b"=== RUN") && l.get(7).is_some_and(|&u| is_js_whitespace(u)))
    {
        return lines;
    }
    let keep = collect_go_keep_names(&lines);
    let mut out = Vec::with_capacity(lines.len());
    let mut cur: Option<Vec<u16>> = None;
    for l in lines {
        if let Some((is_pause, name)) = parse_go_frame_header(&l) {
            let kept = keep.iter().any(|k| k == &name);
            cur = Some(name);
            if kept && !is_pause {
                out.push(l);
            }
            continue;
        }
        let ws = trim_start(&l);
        if starts_with_ascii(ws, b"--- PASS: ")
            || starts_with_ascii(ws, b"--- FAIL: ")
            || starts_with_ascii(ws, b"--- SKIP: ")
        {
            cur = None;
        } else if cur.as_ref().is_some_and(|c| !keep.iter().any(|k| k == c)) {
            continue;
        }
        out.push(l);
    }
    out
}

pub(crate) fn drop_tap_passes(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let ws = trim_start(&lines[i]);
        let is_ok = starts_with_ascii(ws, b"ok ") && {
            let k = 3 + ws[3..].iter().take_while(|&&u| is_ascii_digit(u)).count();
            k > 3 && starts_with_ascii(&ws[k..], b" - ")
        };
        if is_ok {
            if lines
                .get(i + 1)
                .is_some_and(|n| eq_ascii(trim_start(n), b"---"))
            {
                let mut j = i + 2;
                while j < lines.len() && !eq_ascii(trim_start(&lines[j]), b"...") {
                    j += 1;
                }
                i = j + 1;
            } else {
                i += 1;
            }
        } else {
            out.push(lines[i].clone());
            i += 1;
        }
    }
    out
}

fn is_four_space_word(l: &[u16]) -> bool {
    starts_with_ascii(l, b"    ") && l.len() > 4 && !l[4..].iter().any(|&u| is_js_whitespace(u))
}

pub(crate) fn compact_cargo(lines: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        if eq_ascii(l, b"failures:") && lines.get(i + 1).is_some_and(|n| is_four_space_word(n)) {
            while lines.get(i + 1).is_some_and(|n| is_four_space_word(n)) {
                i += 1;
            }
        } else if let Some(replaced) = rewrite_cargo_result_line(l) {
            out.push(replaced);
        } else {
            out.push(l.clone());
        }
        i += 1;
    }
    out
}

fn strip_zero_counters(head: &[u16]) -> Vec<u16> {
    let mut clean_head = Vec::with_capacity(head.len());
    let mut p = 0;
    while p < head.len() {
        if starts_with_ascii(&head[p..], b"; 0 ") {
            let mut q = p + 4;
            while q < head.len() && matches!(head[q], 0x0061..=0x007A | 0x0020) {
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
    clean_head
}

fn rewrite_cargo_result_line(l: &[u16]) -> Option<Vec<u16>> {
    if !starts_with_ascii(l, b"test result: ") {
        return None;
    }
    let mut k = 13;
    while k < l.len() && is_js_word(l[k]) {
        k += 1;
    }
    if k == 13 || !starts_with_ascii(&l[k..], b". ") {
        return None;
    }
    let marker = b"; finished in ";
    // Match the last occurrence so `(.*?)` + `(\S+)$` handles earlier `; finished in ` text.
    let fin_pos = l
        .windows(marker.len())
        .rposition(|w| starts_with_ascii(w, marker))?;
    let time = &l[fin_pos + marker.len()..];
    if time.is_empty() || time.iter().any(|&u| is_js_whitespace(u)) {
        return None;
    }
    let mut out = strip_zero_counters(&l[..fin_pos]);
    out.push(u16::from(b' '));
    out.push(u16::from(b'('));
    out.extend_from_slice(time);
    out.push(u16::from(b')'));
    Some(out)
}
