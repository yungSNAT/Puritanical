// SPDX-License-Identifier: Apache-2.0

//! Removes byte ranges from a file's original text. A file's content for any
//! set of cuts is computed from the original, so a cut is undone by leaving
//! it out.

/// A half-open byte range of a file.
pub type Range = (usize, usize);

/// The start of the line holding byte `at`.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// The end of the line starting at `at`, past its newline.
fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i + 1)
}

fn blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Widens a range to whole lines where only whitespace shares them, then
/// takes one neighbouring blank line where removing the range would leave 2
/// blank lines together or a blank line at the edge of a block or the file,
/// which rustfmt would flag.
fn widen(text: &str, (mut s, mut e): Range) -> Range {
    let ls = line_start(text, s);
    if blank(&text[ls..s]) {
        s = ls;
    }
    let le = line_end(text, e);
    if blank(&text[e..le]) {
        e = le;
    }
    if s != line_start(text, s) || (e != text.len() && e != line_start(text, e)) {
        return (s, e);
    }
    let prev = (s > 0).then(|| &text[line_start(text, s - 1)..s]);
    let next = (e < text.len()).then(|| &text[e..line_end(text, e)]);
    let prev_open = prev.is_none_or(|p| blank(p) || p.trim_end().ends_with('{'));
    if next.is_some_and(blank) && prev_open {
        e = line_end(text, e);
    } else if prev.is_some_and(blank) && next.is_none_or(|n| n.trim_start().starts_with('}')) {
        s = line_start(text, s - 1);
    }
    (s, e)
}

/// Sorts and merges overlapping or touching ranges.
fn merge(mut ranges: Vec<Range>) -> Vec<Range> {
    ranges.sort_unstable();
    let mut out: Vec<Range> = Vec::new();
    for (s, e) in ranges {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// `text` with `ranges` removed (each widened to whole lines where it can be).
#[must_use]
pub fn apply(text: &str, ranges: &[Range]) -> String {
    let widened = merge(ranges.iter().map(|&r| widen(text, r)).collect());
    let widened = merge(widened.into_iter().map(|r| widen(text, r)).collect());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (s, e) in widened {
        out.push_str(&text[at..s]);
        at = e;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::apply;

    fn cut(text: &str, words: &[&str]) -> String {
        let ranges: Vec<_> = words
            .iter()
            .map(|w| {
                let s = text.find(w).expect("word");
                (s, s + w.len())
            })
            .collect();
        apply(text, &ranges)
    }

    #[test]
    fn removes_whole_lines_and_keeps_one_blank_line() {
        let text = "fn a() {}\n\nfn dead() {}\n\nfn b() {}\n";
        assert_eq!(cut(text, &["fn dead() {}"]), "fn a() {}\n\nfn b() {}\n");
    }

    #[test]
    fn leaves_no_blank_line_at_a_block_or_file_edge() {
        let text = "mod m {\n    fn dead() {}\n\n    fn a() {}\n\n    fn tail() {}\n}\n";
        assert_eq!(
            cut(text, &["fn dead() {}", "fn tail() {}"]),
            "mod m {\n    fn a() {}\n}\n"
        );
        assert_eq!(
            cut("fn a() {}\n\nfn dead() {}\n", &["fn dead() {}"]),
            "fn a() {}\n"
        );
        assert_eq!(
            cut("fn dead() {}\n\nfn a() {}\n", &["fn dead() {}"]),
            "fn a() {}\n"
        );
        assert_eq!(cut("fn dead() {}", &["fn dead() {}"]), "");
    }

    #[test]
    fn adjacent_cuts_merge_and_crlf_lines_go_whole() {
        let text = "fn a() {}\r\n\r\nfn x() {}\r\n\r\nfn y() {}\r\n\r\nfn b() {}\r\n";
        assert_eq!(
            cut(text, &["fn x() {}", "fn y() {}"]),
            "fn a() {}\r\n\r\nfn b() {}\r\n"
        );
    }

    #[test]
    fn text_sharing_a_line_stays() {
        let text = "fn a() {} fn dead() {} // note\n";
        assert_eq!(cut(text, &["fn dead() {}"]), "fn a() {}  // note\n");
    }
}
