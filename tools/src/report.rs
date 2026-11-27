// SPDX-License-Identifier: Apache-2.0

//! `report`: measured before-and-after figures as Markdown, with an optional
//! SVG card. Each figure says what was counted and how. Sizes are bytes,
//! never estimated tokens.

use crate::Out;
use crate::readme_art::esc;
use puritanical_core::compress::{compress, scrub};
use std::fmt::Write as _;

/// The `PostToolUse` hook's defaults (`hooks/puritanical-compress-output.js`).
const SCRUB_MIN: usize = 1024;
const MIN_WIN: usize = 64;
const MAX_CHARS: usize = 8000;
const HEAD_LINES: usize = 60;
const TAIL_LINES: usize = 40;

const USAGE: &str = "usage: puritanical-tools report [--audit-before <file>] [--audit-after <file>] [--stats <file>] [--svg <file>] [<tool output file>...]";

/// What the report reads.
#[derive(Default)]
struct Args<'a> {
    audit_before: Option<&'a str>,
    audit_after: Option<&'a str>,
    stats: Option<&'a str>,
    svg: Option<&'a str>,
    outputs: Vec<&'a str>,
}

/// The measured figures.
#[derive(Default)]
struct Report {
    /// File name, bytes before, bytes after.
    outputs: Vec<(String, usize, usize)>,
    /// Code and prose finding lines, before and after.
    audit: [Option<[usize; 2]>; 2],
    /// Ledger path, characters removed, outputs compressed.
    ledger: Option<(String, u64, u64)>,
}

#[must_use]
pub fn run(rest: &[String]) -> Out {
    let Some(args) = parse(rest) else {
        return Out {
            stderr: format!("{USAGE}\n"),
            code: 2,
            ..Out::default()
        };
    };
    match measure(&args) {
        Ok(report) => {
            let card = args
                .svg
                .map(|path| crate::write(std::path::Path::new(path), &svg(&report)));
            match card {
                Some(Err(e)) => Out::fail(e),
                _ => Out {
                    stdout: markdown(&report),
                    ..Out::default()
                },
            }
        }
        Err(e) => Out::fail(e),
    }
}

fn parse(rest: &[String]) -> Option<Args<'_>> {
    let mut args = Args::default();
    let mut it = rest.iter().map(String::as_str);
    while let Some(a) = it.next() {
        let slot = match a {
            "--audit-before" => &mut args.audit_before,
            "--audit-after" => &mut args.audit_after,
            "--stats" => &mut args.stats,
            "--svg" => &mut args.svg,
            _ if a.starts_with("--") => return None,
            _ => {
                args.outputs.push(a);
                continue;
            }
        };
        *slot = Some(it.next()?);
    }
    let any = args.audit_before.or(args.audit_after).or(args.stats);
    (any.is_some() || !args.outputs.is_empty()).then_some(args)
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// The hook only ever sees text, so a file that is not UTF-8 has no figure.
fn read_text(path: &str) -> Result<String, String> {
    String::from_utf8(read(path)?).map_err(|_| format!("{path}: not UTF-8 text"))
}

fn measure(args: &Args) -> Result<Report, String> {
    let mut report = Report::default();
    for path in &args.outputs {
        let text = read_text(path)?;
        let after = compressed(&text).len();
        report
            .outputs
            .push(((*path).to_string(), text.len(), after));
    }
    for (slot, path) in report
        .audit
        .iter_mut()
        .zip([args.audit_before, args.audit_after])
    {
        if let Some(path) = path {
            *slot = Some(findings(&read_text(path)?));
        }
    }
    if let Some(path) = args.stats {
        report.ledger = Some(ledger(path, &read(path)?)?);
    }
    Ok(report)
}

/// `text` after the hook's generic tiers (scrub, then head-and-tail elision),
/// with no command filter and no spill-file note. Unchanged when the win is
/// under the hook's minimum.
#[must_use]
pub fn compressed(text: &str) -> String {
    let original: Vec<u16> = text.encode_utf16().collect();
    let mut t = if original.len() > SCRUB_MIN {
        scrub(&original)
    } else {
        original.clone()
    };
    if t.len() > MAX_CHARS {
        t = compress(&t, MAX_CHARS, HEAD_LINES, TAIL_LINES, None).unwrap_or(t);
    }
    if original.len().saturating_sub(t.len()) < MIN_WIN {
        return text.to_string();
    }
    String::from_utf16_lossy(&t)
}

/// Finding lines (`path:line  [code|prose] ...`) in a `puritanical-audit`
/// output. Fields are separated by 2 spaces, so a path may hold single ones.
fn findings(text: &str) -> [usize; 2] {
    let mut n = [0, 0];
    for line in text.lines() {
        let Some((at, rest)) = line.split_once("  [") else {
            continue;
        };
        if at
            .rsplit_once(':')
            .is_none_or(|(_, l)| l.parse::<u32>().is_err())
        {
            continue;
        }
        if rest.starts_with("code]") {
            n[0] += 1;
        } else if rest.starts_with("prose]") {
            n[1] += 1;
        }
    }
    n
}

/// The hook's savings ledger (`.puritanical-compress-stats.json`), named by
/// its file name alone, so a shared report carries no home directory.
fn ledger(path: &str, bytes: &[u8]) -> Result<(String, u64, u64), String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    let field = |k| v.get(k).and_then(serde_json::Value::as_u64);
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match (field("savedChars"), field("events")) {
        (Some(saved), Some(events)) => Ok((name.to_string(), saved, events)),
        _ => Err(format!(
            "{path}: not a compressor ledger (needs whole-number savedChars and events)"
        )),
    }
}

/// `1234567` as `1,234,567`.
fn commas(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The share saved, rounded down so it never claims more than was saved; a
/// growth shows as negative.
fn saved(before: usize, after: usize) -> String {
    if before == 0 {
        return "n/a".into();
    }
    let tenths = ((before as f64 - after as f64) * 1000.0 / before as f64).floor();
    format!("{:.1}%", tenths / 10.0)
}

/// `s` as a Markdown code span that nothing in it can close or break out of:
/// control characters become U+FFFD and the fence outruns any backtick run.
fn code(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect::<String>()
        .replace('|', "\\|");
    let run = s.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(run + 1);
    let pad = if run == 0 { "" } else { " " };
    format!("{fence}{pad}{s}{pad}{fence}")
}

fn markdown(r: &Report) -> String {
    let mut md = String::from("# Puritanical report\n");
    if !r.outputs.is_empty() {
        md += "\n## Tool output\n\nUTF-8 bytes of each file before and after the PostToolUse compressor's generic tiers, run through `puritanical-core` with the hook's defaults: scrub above 1,024 UTF-16 code units; above 8,000, keep the first 60 and last 40 lines and error-like lines, or the first and last 4,000 units of an output of 100 lines or fewer; keep the original unless at least 64 units go. No command filter or spill-file note is applied. File names appear as given. Bytes, not tokens.\n\n| File | Bytes before | Bytes after | Saved |\n|---|---:|---:|---:|\n";
        for (name, before, after) in &r.outputs {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} |",
                code(name),
                commas(*before as u64),
                commas(*after as u64),
                saved(*before, *after)
            );
        }
        let (before, after) = totals(r);
        let _ = writeln!(
            md,
            "| All {} | {} | {} | {} |",
            r.outputs.len(),
            commas(before as u64),
            commas(after as u64),
            saved(before, after)
        );
    }
    audit(&mut md, r.audit);
    if let Some((name, chars, events)) = &r.ledger {
        let _ = write!(
            md,
            "\n## Compressor ledger\n\n{} records {} characters (UTF-16 code units, as the hook counts them) removed from {} tool outputs.\n",
            code(name),
            commas(*chars),
            commas(*events)
        );
    }
    md
}

/// The audit table: one column for each audit given.
fn audit(md: &mut String, [b, a]: [Option<[usize; 2]>; 2]) {
    let cols: Vec<(&str, [usize; 2])> = [("Before", b), ("After", a)]
        .into_iter()
        .filter_map(|(h, n)| Some((h, n?)))
        .collect();
    if cols.is_empty() {
        return;
    }
    *md += "\n## Audit findings\n\nFinding lines (`path:line  [code|prose]`) counted in each `puritanical-audit` output.\n\n| Kind |";
    for (h, _) in &cols {
        let _ = write!(md, " {h} |");
    }
    let _ = writeln!(md, "\n|---|{}", "---:|".repeat(cols.len()));
    for (i, kind) in ["code", "prose", "all"].iter().enumerate() {
        let _ = write!(md, "| {kind} |");
        for (_, n) in &cols {
            let _ = write!(md, " {} |", if i == 2 { n[0] + n[1] } else { n[i] });
        }
        md.push('\n');
    }
}

fn totals(r: &Report) -> (usize, usize) {
    r.outputs
        .iter()
        .fold((0, 0), |(b, a), (_, before, after)| (b + before, a + after))
}

// Palette (`mermaid-palette`): black card, dark gold frame, platinum titles,
// ivory text, Tyrian purple for the before bar, crimson for the after bar.
const W: u32 = 600;
const BAR: f64 = 380.0;

/// One titled pair of bars, scaled to the larger value.
fn bars(svg: &mut String, y: &mut u32, title: &str, unit: &str, pair: [usize; 2]) {
    let _ = write!(
        svg,
        "<text x=\"24\" y=\"{}\" fill=\"#E5E4E2\" font-size=\"15\">{}</text>",
        *y + 18,
        esc(title)
    );
    *y += 28;
    let max = pair[0].max(pair[1]).max(1) as f64;
    for (label, fill, n) in [
        ("Before", "#66023C", pair[0]),
        ("After", "#990024", pair[1]),
    ] {
        let w = (n as f64 / max * BAR).round();
        let _ = write!(
            svg,
            "<text x=\"24\" y=\"{}\" fill=\"#FFFFF0\" font-size=\"13\">{label}: {} {unit}</text><rect x=\"196\" y=\"{}\" width=\"{w}\" height=\"16\" rx=\"3\" fill=\"{fill}\" stroke=\"#FFFFF0\" stroke-width=\"1\"/>",
            *y + 13,
            commas(n as u64),
            *y + 1
        );
        *y += 24;
    }
    *y += 8;
}

fn svg(r: &Report) -> String {
    let mut body = String::new();
    let mut y = 52;
    if !r.outputs.is_empty() {
        let (before, after) = totals(r);
        let title = format!("Tool output: {} saved", saved(before, after));
        bars(&mut body, &mut y, &title, "bytes", [before, after]);
    }
    let [b, a] = r.audit;
    if let (Some(b), Some(a)) = (b, a) {
        bars(
            &mut body,
            &mut y,
            "Audit findings",
            "findings",
            [b[0] + b[1], a[0] + a[1]],
        );
    }
    if let Some((_, chars, events)) = &r.ledger {
        let _ = write!(
            body,
            "<text x=\"24\" y=\"{}\" fill=\"#FFFFF0\" font-size=\"13\">Compressor ledger: {} characters removed from {} outputs</text>",
            y + 13,
            commas(*chars),
            commas(*events)
        );
        y += 28;
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{W}\" height=\"{y}\" viewBox=\"0 0 {W} {y}\" font-family=\"-apple-system,BlinkMacSystemFont,'Segoe UI','Noto Sans',Helvetica,Arial,sans-serif\" role=\"img\" aria-label=\"Puritanical report\"><rect x=\"1\" y=\"1\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"#000000\" stroke=\"#9B7735\" stroke-width=\"2\"/><text x=\"24\" y=\"34\" fill=\"#E5E4E2\" font-size=\"18\" font-weight=\"600\">Puritanical report</text>{body}</svg>\n",
        W - 2,
        y - 2
    )
}

#[cfg(test)]
mod tests {
    use super::{Report, markdown, saved};

    #[test]
    fn a_pipe_in_a_file_name_cannot_break_the_table() {
        let r = Report {
            outputs: vec![("a|b.log".into(), 2000, 1000)],
            ..Report::default()
        };
        assert!(markdown(&r).contains(r"| `a\|b.log` | 2,000 | 1,000 | 50.0% |"));
    }

    #[test]
    fn a_backtick_or_newline_in_a_file_name_stays_inside_its_code_span() {
        let r = Report {
            outputs: vec![
                ("x`[here](https://example.invalid)``.log".into(), 9, 9),
                ("a\nb.log".into(), 9, 9),
            ],
            ..Report::default()
        };
        let md = markdown(&r);
        assert!(
            md.contains("| ``` x`[here](https://example.invalid)``.log ``` | 9 |"),
            "{md}"
        );
        assert!(md.contains("| `a\u{FFFD}b.log` | 9 |"), "{md}");
    }

    #[test]
    fn the_saved_figure_rounds_down_and_shows_growth() {
        assert_eq!(saved(20_000, 1), "99.9%");
        assert_eq!(saved(1000, 1001), "-0.1%");
        assert_eq!(saved(0, 0), "n/a");
    }
}
