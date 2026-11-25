#![forbid(unsafe_code)]

//! Port of pure output-compression functions from `hooks/puritanical-compress-output.js`.

mod credentials;
mod json;
mod salvage;
mod scrub;

pub use credentials::contains_credential;
pub use json::{minify_json, strip_ansi};
pub use salvage::{find_salvage_match, salvage_line};
pub use scrub::scrub;

use crate::utf16::{
    find_ascii_ci, join_u16, push_ascii, push_commas, push_usize, safe_prefix, safe_suffix,
    split_lf,
};

pub const DEFAULT_MAX_CHARS: usize = 8000;
pub const MAX_SALVAGED: usize = 12;
pub const MAX_SALVAGE_LINE: usize = 300;
pub const REPEAT_MIN: usize = 4;
pub const NOISE_MIN: usize = 6;

const UNSAFE_TOOL_SUBSTRINGS: &[&[u8]] = &[
    b"read",
    b"view",
    b"file",
    b"edit",
    b"write",
    b"patch",
    b"replace",
    b"insert",
    b"lsp",
    b"symbol",
    b"definition",
    b"declaration",
    b"implementation",
    b"reference",
    b"hover",
    b"content",
    b"source",
    b"blob",
    b"search_code",
];

#[must_use]
pub fn is_ascii_alnum(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0041..=0x005A | 0x0061..=0x007A)
}

#[must_use]
pub fn is_ascii_digit(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039)
}

#[must_use]
pub fn is_ascii_hex(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0041..=0x0046 | 0x0061..=0x0066)
}

#[must_use]
pub fn is_ascii_upper(u: u16) -> bool {
    matches!(u, 0x0041..=0x005A)
}

#[must_use]
pub fn unsafe_tool_name(name: &[u16]) -> bool {
    UNSAFE_TOOL_SUBSTRINGS
        .iter()
        .any(|needle| find_ascii_ci(name, needle).is_some())
}

#[must_use]
pub fn duplicate_marker(tool_name: &[u16], text: &[u16], is_earlier: bool) -> Vec<u16> {
    let lines = split_lf(text);
    let mut out = Vec::with_capacity(512);
    push_ascii(&mut out, "[puritanical: output byte-identical to ");
    let prefix = if is_earlier {
        "an earlier "
    } else {
        "the previous "
    };
    push_ascii(&mut out, prefix);
    out.extend_from_slice(tool_name);
    push_ascii(&mut out, " result \u{2014} ");
    push_commas(&mut out, text.len());
    push_ascii(&mut out, " chars / ");
    push_usize(&mut out, lines.len());
    push_ascii(&mut out, " lines, unchanged. First lines:]");
    append_first_lines(&mut out, &lines);
    out
}

fn append_first_lines(out: &mut Vec<u16>, lines: &[&[u16]]) {
    for line in lines.iter().take(5) {
        out.push(0x000A);
        if line.len() > MAX_SALVAGE_LINE {
            out.extend_from_slice(safe_prefix(line, MAX_SALVAGE_LINE - 1));
            out.push(0x2026);
        } else {
            out.extend_from_slice(line);
        }
    }
}

fn push_spill_recovery(out: &mut Vec<u16>, spill_path: Option<&[u16]>) {
    if let Some(sp) = spill_path
        && !sp.is_empty()
    {
        push_ascii(out, " Full output: ");
        out.extend_from_slice(sp);
        push_ascii(out, " (grep it, do not re-run)");
    }
}

fn compress_char_cut(
    text: &[u16],
    max_chars: usize,
    spill_path: Option<&[u16]>,
) -> Option<Vec<u16>> {
    let keep = max_chars / 2;
    let elided = text.len().saturating_sub(keep.saturating_mul(2));
    if elided == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(max_chars.min(text.len()) + 160);
    out.extend_from_slice(safe_prefix(text, keep));
    push_ascii(&mut out, "\n... [puritanical: elided ");
    push_commas(&mut out, elided);
    push_ascii(&mut out, " chars from the middle.");
    push_spill_recovery(&mut out, spill_path);
    push_ascii(&mut out, "] ...\n");
    out.extend_from_slice(safe_suffix(text, keep));
    Some(out)
}

fn build_elision_marker(
    elided_lines: usize,
    head_lines: usize,
    tail_lines: usize,
    salvaged_len: usize,
    spill_path: Option<&[u16]>,
) -> Vec<u16> {
    let mut marker = Vec::with_capacity(160);
    push_ascii(&mut marker, "... [puritanical: elided ");
    push_commas(&mut marker, elided_lines);
    push_ascii(&mut marker, " lines, kept first ");
    push_usize(&mut marker, head_lines);
    push_ascii(&mut marker, ", last ");
    push_usize(&mut marker, tail_lines);
    if salvaged_len > 0 {
        push_ascii(&mut marker, ", and ");
        push_usize(&mut marker, salvaged_len);
        push_ascii(&mut marker, " error-like line(s) below");
    }
    marker.push(u16::from(b'.'));
    push_spill_recovery(&mut marker, spill_path);
    push_ascii(&mut marker, "] ...");
    marker
}

#[must_use]
pub fn compress(
    text: &[u16],
    max_chars: usize,
    head_lines: usize,
    tail_lines: usize,
    spill_path: Option<&[u16]>,
) -> Option<Vec<u16>> {
    let lines = split_lf(text);
    if lines.len() <= head_lines.saturating_add(tail_lines) {
        return compress_char_cut(text, max_chars, spill_path);
    }

    let head = &lines[..head_lines];
    // Matches `lines.slice(-tailLines)` when tail_lines > 0 and full slice `lines.slice(-0)` when 0.
    let tail_start = if tail_lines == 0 {
        0
    } else {
        lines.len().saturating_sub(tail_lines)
    };
    let tail = &lines[tail_start..];
    let middle_end = lines.len().saturating_sub(tail_lines);
    let middle = &lines[head_lines..middle_end];

    let mut salvaged: Vec<Vec<u16>> = Vec::new();
    for &line in middle {
        if salvaged.len() >= MAX_SALVAGED {
            break;
        }
        if let Some(cand) = salvage_line(line) {
            salvaged.push(cand);
        }
    }

    let marker = build_elision_marker(
        middle.len(),
        head_lines,
        tail_lines,
        salvaged.len(),
        spill_path,
    );
    let mut parts: Vec<&[u16]> = Vec::with_capacity(head.len() + 1 + salvaged.len() + tail.len());
    parts.extend_from_slice(head);
    parts.push(&marker);
    for s in &salvaged {
        parts.push(s.as_slice());
    }
    parts.extend_from_slice(tail);
    let out = join_u16(&parts, 0x000A);
    (out.len() < text.len()).then_some(out)
}

fn build_bound_marker(output_limit: usize, spill_path: Option<&[u16]>) -> Vec<u16> {
    let mut marker = Vec::with_capacity(160);
    push_ascii(
        &mut marker,
        "... [puritanical: rebound compressed output to ",
    );
    push_commas(&mut marker, output_limit);
    push_ascii(&mut marker, " chars.");
    push_spill_recovery(&mut marker, spill_path);
    push_ascii(&mut marker, "] ...");
    if marker.len() > output_limit - 2 {
        let mut truncated = safe_prefix(&marker, output_limit - 7).to_vec();
        push_ascii(&mut truncated, "] ...");
        return truncated;
    }
    marker
}

fn collect_bound_salvage(text: &[u16], output_limit: usize, marker: Vec<u16>) -> Vec<u16> {
    let mut middle: Vec<Vec<u16>> = vec![marker];
    let mut middle_joined_len = middle[0].len();
    let reserve = (output_limit / 4).min(2050);

    for line in split_lf(text) {
        if middle.len() > MAX_SALVAGED {
            break;
        }
        let Some(candidate) = salvage_line(line) else {
            continue;
        };
        if middle_joined_len + candidate.len() + 1 > output_limit.saturating_sub(reserve) {
            break;
        }
        if !middle.iter().any(|existing| existing == &candidate) {
            middle_joined_len += 1 + candidate.len();
            middle.push(candidate);
        }
    }
    let middle_refs: Vec<&[u16]> = middle.iter().map(Vec::as_slice).collect();
    join_u16(&middle_refs, 0x000A)
}

#[must_use]
pub fn bound_for_opencode(
    text: &[u16],
    spill_path: Option<&[u16]>,
    max_chars: usize,
) -> Option<Vec<u16>> {
    let output_limit = if max_chars == 0 {
        DEFAULT_MAX_CHARS
    } else {
        max_chars
    };
    if text.len() <= output_limit {
        return None;
    }
    if output_limit < 7 {
        return Some(safe_prefix(text, output_limit).to_vec());
    }

    let marker = build_bound_marker(output_limit, spill_path);
    let centre = collect_bound_salvage(text, output_limit, marker);
    let remaining = output_limit.saturating_sub(centre.len() + 2);
    let head = safe_prefix(text, remaining.div_ceil(2));
    let tail = safe_suffix(text, remaining / 2);

    let mut out = Vec::with_capacity(head.len() + centre.len() + tail.len() + 2);
    out.extend_from_slice(head);
    out.push(0x000A);
    out.extend_from_slice(&centre);
    out.push(0x000A);
    out.extend_from_slice(tail);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        bound_for_opencode, compress, contains_credential, duplicate_marker, minify_json,
        salvage_line, scrub, unsafe_tool_name,
    };
    use crate::utf16::{str_from_u16_lossy, u16_from_str};

    #[test]
    fn basic_compress_and_scrub() {
        assert!(unsafe_tool_name(&u16_from_str("mcp__fs__read_file")));
        assert!(!unsafe_tool_name(&u16_from_str("Bash")));
        assert!(!contains_credential(&u16_from_str(
            "Authorization: token required"
        )));
        assert!(contains_credential(&u16_from_str(
            "Authorization: Bearer abcdefgh1234"
        )));
        assert_eq!(
            str_from_u16_lossy(&minify_json(&u16_from_str("{\n  \"a\": 1\n}")).unwrap()),
            r#"{"a":1}"#
        );
        assert!(minify_json(&u16_from_str("{\n  \"a\": 01\n}")).is_none());
        let dup = str_from_u16_lossy(&duplicate_marker(
            &u16_from_str("Bash"),
            &u16_from_str("hi\nthere"),
            false,
        ));
        assert!(dup.contains("the previous Bash result"));
        assert!(salvage_line(&u16_from_str("this error-like placeholder")).is_none());
        let big = u16_from_str(
            &(0..60)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let comp = compress(&big, 100, 3, 3, None).unwrap();
        assert!(comp.len() < big.len());
        let _ = compress(&big, 20, 5, 0, None);
        let sc = scrub(&u16_from_str("a  \n\n\n\nb"));
        assert_eq!(str_from_u16_lossy(&sc), "a\n\nb");
        let bnd = bound_for_opencode(&big, None, 120).unwrap();
        assert!(bnd.len() <= 120);
    }
}
