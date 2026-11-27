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

fn measure(args: &Args) -> Result<Report, String> {
    let mut report = Report::default();
    for path in &args.outputs {
        let bytes = read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let out = compressed(&text);
        // Unchanged stays the file's own size, even when it is not UTF-8.
        let after = if out == text { bytes.len() } else { out.len() };
        report
            .outputs
            .push(((*path).to_string(), bytes.len(), after));
    }
    for (slot, path) in report
        .audit
        .iter_mut()
        .zip([args.audit_before, args.audit_after])
    {
        if let Some(path) = path {
            *slot = Some(findings(&String::from_utf8_lossy(&read(path)?)));
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

/// Finding lines (`path:line  [code|prose] ...`) in a `puritanical-audit` output.
fn findings(text: &str) -> [usize; 2] {
    let mut n = [0, 0];
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if !words.next().is_some_and(|w| w.contains(':')) {
            continue;
        }
        match words.next() {
            Some("[code]") => n[0] += 1,
            Some("[prose]") => n[1] += 1,
            _ => {}
        }
    }
    n
}

/// The hook's savings ledger (`.puritanical-compress-stats.json`).
fn ledger(path: &str, bytes: &[u8]) -> Result<(String, u64, u64), String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    let field = |k| v.get(k).and_then(serde_json::Value::as_u64);
    match (field("savedChars"), field("events")) {
        (Some(saved), Some(events)) => Ok((path.to_string(), saved, events)),
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

fn saved(before: usize, after: usize) -> String {
    if before == 0 {
        return "0.0%".into();
    }
    format!(
        "{:.1}%",
        before.saturating_sub(after) as f64 * 100.0 / before as f64
    )
}

fn markdown(r: &Report) -> String {
    let mut md = String::from("# Puritanical report\n");
    if !r.outputs.is_empty() {
        md += "\n## Tool output\n\nUTF-8 bytes of each file before and after the PostToolUse compressor's generic tiers, run through `puritanical-core` with the hook's defaults (scrub above 1,024 characters; above 8,000, keep the first 60 and last 40 lines and error-like lines). No command filter or spill-file note is applied. Bytes, not tokens.\n\n| File | Bytes before | Bytes after | Saved |\n|---|---:|---:|---:|\n";
        for (name, before, after) in &r.outputs {
            let name = name.replace('|', "\\|");
            let _ = writeln!(
                md,
                "| `{name}` | {} | {} | {} |",
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
    if let Some((path, chars, events)) = &r.ledger {
        let _ = write!(
            md,
            "\n## Compressor ledger\n\n`{path}` records {} characters (UTF-16 code units, as the hook counts them) removed from {} tool outputs.\n",
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
        let w = (n as f64 / max * BAR).round().max(1.0);
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
            "lines",
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
    use super::{Report, markdown};

    #[test]
    fn a_pipe_in_a_file_name_cannot_break_the_table() {
        let r = Report {
            outputs: vec![("a|b.log".into(), 2000, 1000)],
            ..Report::default()
        };
        assert!(markdown(&r).contains(r"| `a\|b.log` | 2,000 | 1,000 | 50.0% |"));
    }
}
