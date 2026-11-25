// SPDX-License-Identifier: Apache-2.0 AND MIT

//! The trim panel.

use super::{Lang, PLAIN, Theme, anim_n, esc, fit, mono, n, r1, svg, tspans, win};

// ── Trim: what the tool-output compressor does to one oversized output ───────
const T_Y0: f64 = 76.0;
const T_STEP: f64 = 19.5;
const T_D: f64 = 9.0;

fn t_at(i: usize) -> f64 {
    0.7 + i as f64 * (5.0 / 12.0)
}

/// How a raw segment leaves: 0 stays, 1 is an ANSI code (fades to 0.12), 2 is elided (0.35).
type Seg<'a> = (&'a str, &'a str, u8);

fn trim_left(t: &Theme) -> Vec<Vec<Seg<'static>>> {
    let ansi = |s: &'static str| (s, t.accent, 1);
    let mut left = vec![
        vec![
            ansi("\\e[1m"),
            ("> chisle@3.12.1 test", t.soft, 0),
            ansi("\\e[0m"),
        ],
        vec![
            ansi("\\e[32m"),
            ("✓", t.green, 0),
            ansi("\\e[0m"),
            (" retry backs off", t.soft, 0),
        ],
    ];
    for _ in 0..5 {
        left.push(vec![
            ansi("\\e[32m"),
            ("✓", t.green, 2),
            ansi("\\e[0m"),
            (" retry backs off", t.soft, 2),
        ]);
    }
    left.extend([
        vec![("✓", t.green, 0), (" strips colour codes", t.soft, 0)],
        vec![("✓", t.green, 0), (" collapses repeats", t.soft, 0)],
        vec![("  … 172 more passing lines …", t.faint, 2)],
        vec![("✗ spill file is owner-only", t.red, 0)],
        vec![("  AssertionError: 0o644 !== 0o600", t.red, 0)],
        vec![("ℹ tests 246  pass 245  fail 1", t.soft, 0)],
    ]);
    left
}

pub(super) fn trim(t: &Theme, l: &Lang) -> String {
    let (w, h, d) = (880.0, 396.0, T_D);
    let mut o = vec![
        win(t, 16.0, 16.0, 416.0, 330.0, "bash · npm test"),
        win(t, 448.0, 16.0, 416.0, 330.0, l.reads),
        trim_scan(t),
        format!("<g{}>", mono(12.5)),
    ];
    o.extend(
        trim_left(t)
            .iter()
            .enumerate()
            .map(|(i, segs)| trim_raw(i, segs)),
    );
    for (row, (li, segs)) in trim_right(t, l).iter().enumerate() {
        let at = t_at(*li) + 0.15;
        o.push(format!(
            "<text x=\"462\" y=\"{}\">{}{}</text>",
            n(r1(T_Y0 + row as f64 * T_STEP)),
            tspans(segs),
            anim_n(
                "opacity",
                &[(at, 0.0), (at + 0.3, 1.0), (d - 0.5, 1.0), (d - 0.2, 0.0)],
                d,
                PLAIN
            )
        ));
    }
    o.push("</g>".into());
    o.extend(trim_foot(t, l, w));
    svg(
        t,
        w,
        h,
        "Animated illustration of the Puritanical tool-output compressor: raw npm test output with colour codes, six identical lines and 172 passing lines becomes a short version with the codes stripped, the repeats collapsed, the middle elided with the full output saved to a file, and the failing test and its error kept. On real sessions, oversized output shrinks by about 48 percent on average.",
        &o,
        "",
    )
}

/// The highlight that scans down the raw output.
fn trim_scan(t: &Theme) -> String {
    let d = T_D;
    format!(
        "<rect x=\"17\" y=\"{}\" width=\"414\" height=\"20\" fill=\"{}\" fill-opacity=\"0.13\" opacity=\"0\">{}{}</rect>",
        n(T_Y0 - 14.0),
        t.fill,
        anim_n(
            "y",
            &[(0.7, T_Y0 - 14.0), (5.7, T_Y0 - 14.0 + 12.0 * T_STEP)],
            d,
            PLAIN
        ),
        anim_n(
            "opacity",
            &[(0.69, 0.0), (0.7, 1.0), (5.7, 1.0), (5.71, 0.0)],
            d,
            PLAIN
        )
    )
}

/// What Claude reads: each line with the raw line it came from.
fn trim_right(t: &Theme, l: &Lang) -> [(usize, Vec<(&'static str, &'static str)>); 10] {
    [
        (0, vec![("> chisle@3.12.1 test", t.soft)]),
        (1, vec![("✓", t.green), (" retry backs off", t.soft)]),
        (
            6,
            vec![("... [puritanical: line repeated 6×] ...", t.accent)],
        ),
        (7, vec![("✓", t.green), (" strips colour codes", t.soft)]),
        (8, vec![("✓", t.green), (" collapses repeats", t.soft)]),
        (9, vec![("... [puritanical: elided 172 lines,", t.accent)]),
        (9, vec![("     full output saved to a file] ...", t.accent)]),
        (
            10,
            vec![("✗ spill file is owner-only", t.red), (l.kept, t.green)],
        ),
        (11, vec![("  AssertionError: 0o644 !== 0o600", t.red)]),
        (12, vec![("ℹ tests 246  pass 245  fail 1", t.soft)]),
    ]
}

/// Raw line `i`, its removed segments fading as the scan passes.
fn trim_raw(i: usize, segs: &[Seg]) -> String {
    let at = t_at(i);
    let spans = segs
        .iter()
        .map(|&(s, c, rm)| {
            let end = match rm {
                1 => 0.12,
                2 => 0.35,
                _ => 1.0,
            };
            let fade = if rm == 0 {
                String::new()
            } else {
                anim_n("fill-opacity", &[(at, 1.0), (at + 0.3, end)], T_D, PLAIN)
            };
            format!(
                "<tspan fill=\"{c}\" fill-opacity=\"{}\">{}{fade}</tspan>",
                n(end),
                esc(s)
            )
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "<text x=\"28\" y=\"{}\">{spans}</text>",
        n(r1(T_Y0 + i as f64 * T_STEP))
    )
}

/// The 2 footers that swap as the scan finishes.
fn trim_foot(t: &Theme, l: &Lang, w: f64) -> [String; 2] {
    let d = T_D;
    [
        format!(
            "<text x=\"{}\" y=\"376\" font-size=\"{}\" fill=\"{}\" text-anchor=\"middle\" opacity=\"0\">{}{}</text>",
            n(w / 2.0),
            fit(l.foot1, 15.5, 820.0),
            t.muted,
            esc(l.foot1),
            anim_n(
                "opacity",
                &[
                    (0.0, 1.0),
                    (6.0, 1.0),
                    (6.2, 0.0),
                    (d - 0.3, 0.0),
                    (d - 0.1, 1.0)
                ],
                d,
                PLAIN
            )
        ),
        format!(
            "<text x=\"{}\" y=\"376\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\" text-anchor=\"middle\">{}{}</text>",
            n(w / 2.0),
            fit(l.foot2, 15.5, 760.0),
            t.accent,
            esc(l.foot2),
            anim_n(
                "opacity",
                &[(6.1, 0.0), (6.4, 1.0), (d - 0.5, 1.0), (d - 0.2, 0.0)],
                d,
                PLAIN
            )
        ),
    ]
}
