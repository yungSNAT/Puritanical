#![forbid(unsafe_code)]

//! Port of `hooks/puritanical-config.js` pure helpers (`filterSections`,
//! `isOldStatusLine`, `isOwnedStatusLine`).

use crate::utf16::{eq_ascii, is_line_terminator, starts_with_ascii, trim};

pub const DEFAULT_PROSE_HEADINGS: &[&[u8]] =
    &[b"Prose: Maximum Signal Per Token", b"Output Format"];
pub const DEFAULT_CODE_HEADINGS: &[&[u8]] = &[
    b"Code: The Efficiency Ladder",
    b"Code Rules",
    b"Context Diet: Read Less Into the Window",
];

fn line_end_after(s: &[u16], start: usize) -> usize {
    let mut i = start;
    while i < s.len() && !is_line_terminator(s[i]) {
        i += 1;
    }
    i
}

/// Checks if `s[i..]` begins with `## <at least one non-terminator char>`.
fn is_h2_line_start(s: &[u16], i: usize) -> Option<(usize, usize)> {
    let at_line_start = i == 0 || is_line_terminator(s[i - 1]);
    if !at_line_start || !starts_with_ascii(&s[i..], b"## ") {
        return None;
    }
    let heading_start = i + 3;
    let heading_end = line_end_after(s, heading_start);
    if heading_end > heading_start {
        Some((heading_start, heading_end))
    } else {
        None
    }
}

fn collect_h2_splits(content: &[u16]) -> Vec<usize> {
    let mut split_points = Vec::new();
    for i in 1..content.len() {
        if is_h2_line_start(content, i).is_some() && split_points.last().copied() != Some(i) {
            split_points.push(i);
        }
    }
    split_points
}

fn should_drop_chunk(
    chunk: &[u16],
    prose: bool,
    code: bool,
    prose_headings: Option<&[&[u16]]>,
    code_headings: Option<&[&[u16]]>,
) -> bool {
    let Some(title) = find_first_h2_heading(chunk) else {
        return false;
    };
    let t = trim(title);
    let in_prose = match prose_headings {
        Some(list) => list.contains(&t),
        None => DEFAULT_PROSE_HEADINGS.iter().any(|&h| eq_ascii(t, h)),
    };
    let in_code = match code_headings {
        Some(list) => list.contains(&t),
        None => DEFAULT_CODE_HEADINGS.iter().any(|&h| eq_ascii(t, h)),
    };
    (!prose && in_prose) || (!code && in_code)
}

/// Drops `## Heading` sections whose trimmed title belongs to a disabled group.
/// Returns `None` when nothing changes (caller keeps the original JS string).
#[must_use]
pub fn filter_sections(
    content: &[u16],
    prose: bool,
    code: bool,
    prose_headings: Option<&[&[u16]]>,
    code_headings: Option<&[&[u16]]>,
) -> Option<Vec<u16>> {
    if content.is_empty() || (prose && code) {
        return None;
    }
    let split_points = collect_h2_splits(content);
    let mut out = Vec::with_capacity(content.len());
    let mut changed = false;
    let mut start = 0usize;
    for end in split_points
        .into_iter()
        .chain(std::iter::once(content.len()))
    {
        let chunk = &content[start..end];
        start = end;
        if should_drop_chunk(chunk, prose, code, prose_headings, code_headings) {
            changed = true;
        } else {
            out.extend_from_slice(chunk);
        }
    }
    changed.then_some(out)
}

fn find_first_h2_heading(chunk: &[u16]) -> Option<&[u16]> {
    for i in 0..chunk.len() {
        if let Some((h_start, h_end)) = is_h2_line_start(chunk, i) {
            return Some(&chunk[h_start..h_end]);
        }
    }
    None
}

fn matches_statusline_command(cmd: &[u16], script_base: &[u8]) -> bool {
    let sh_suffix_len = script_base.len() + 3; // `.sh`
    if let Some(after_bash) = cmd.strip_prefix(&[
        b'b' as u16,
        b'a' as u16,
        b's' as u16,
        b'h' as u16,
        b' ' as u16,
        b'"' as u16,
    ]) && let Some(inner) = after_bash.strip_suffix(&[b'"' as u16])
        && !inner.contains(&(b'"' as u16))
        && inner.len() > sh_suffix_len
    {
        let sep_idx = inner.len() - sh_suffix_len - 1;
        let sep = inner[sep_idx];
        if (sep == u16::from(b'/') || sep == u16::from(b'\\'))
            && starts_with_ascii(&inner[sep_idx + 1..], script_base)
            && eq_ascii(&inner[sep_idx + 1 + script_base.len()..], b".sh")
        {
            return true;
        }
    }

    let ps1_rest = if let Some(r) =
        strip_ascii_prefix(cmd, b"pwsh -NoProfile -ExecutionPolicy Bypass -File \"")
    {
        r
    } else if let Some(r) = strip_ascii_prefix(
        cmd,
        b"powershell -NoProfile -ExecutionPolicy Bypass -File \"",
    ) {
        r
    } else {
        return false;
    };

    let ps1_suffix_len = script_base.len() + 4; // `.ps1`
    if let Some(inner) = ps1_rest.strip_suffix(&[b'"' as u16])
        && !inner.contains(&(b'"' as u16))
        && inner.len() > ps1_suffix_len
    {
        let sep_idx = inner.len() - ps1_suffix_len - 1;
        let sep = inner[sep_idx];
        return (sep == u16::from(b'/') || sep == u16::from(b'\\'))
            && starts_with_ascii(&inner[sep_idx + 1..], script_base)
            && eq_ascii(&inner[sep_idx + 1 + script_base.len()..], b".ps1");
    }
    false
}

fn strip_ascii_prefix<'a>(s: &'a [u16], prefix: &[u8]) -> Option<&'a [u16]> {
    if starts_with_ascii(s, prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[must_use]
pub fn is_old_statusline_command(cmd: &[u16]) -> bool {
    matches_statusline_command(cmd, b"chisle-statusline")
}

#[must_use]
pub fn is_owned_statusline_command(cmd: &[u16]) -> bool {
    matches_statusline_command(cmd, b"puritanical-statusline")
        || matches_statusline_command(cmd, b"chisle-statusline")
}

#[cfg(test)]
mod tests {
    use super::{filter_sections, is_old_statusline_command, is_owned_statusline_command};
    use crate::utf16::{str_from_u16_lossy, u16_from_str};

    #[test]
    fn filter_sections_crlf_and_custom() {
        let doc = u16_from_str("Pre\r\n## Prose\r\nA\r\n## Keep\r\nB");
        let p = u16_from_str("Prose");
        let c = u16_from_str("Code");
        let out = filter_sections(&doc, false, true, Some(&[&p]), Some(&[&c])).unwrap();
        assert_eq!(str_from_u16_lossy(&out), "Pre\r\n## Keep\r\nB");
    }

    #[test]
    fn statusline_patterns() {
        assert!(is_owned_statusline_command(&u16_from_str(
            r#"pwsh -NoProfile -ExecutionPolicy Bypass -File "C:\x\puritanical-statusline.ps1""#
        )));
        assert!(is_old_statusline_command(&u16_from_str(
            "bash \"/home/u/.claude/chisle-statusline.sh\""
        )));
        assert!(!is_owned_statusline_command(&u16_from_str(
            "bash \"puritanical-statusline.sh\""
        )));
    }
}
