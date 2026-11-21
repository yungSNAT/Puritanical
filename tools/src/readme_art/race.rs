// SPDX-License-Identifier: MIT

//! The race, trim and filters panels.

use super::{
    EASE, Lang, PLAIN, Theme, anim_n, esc, fit, fit_as, grouped, js, mono, n, r1, shown, svg,
    text_w, tspans, win,
};

// ── Race: one prompt, four rulesets, tokens streamed at the same speed ────────
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
pub(super) fn race(t: &Theme, l: &Lang) -> String {
    let (w, h, d, x, maxw, t0, run) = (880.0, 304.0, 9.0, 150.0, 560.0, 0.6, 4.4);
    let arms = [
        ("Chisle", 567.0),
        ("ponytail", 740.0),
        ("caveman", 1018.0),
        (l.bare, 1571.0),
    ];
    let max = arms.iter().map(|a| a.1).fold(f64::NEG_INFINITY, f64::max);
    let mut o = vec![
        format!(
            "<text x=\"28\" y=\"42\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\">{}</text>",
            fit(l.race_title, 19.0, 420.0),
            t.text,
            esc(l.race_title)
        ),
        format!(
            "<text x=\"{}\" y=\"42\" font-size=\"{}\" fill=\"{}\" text-anchor=\"end\">{}</text>",
            n(w - 28.0),
            fit(l.race_note, 13.0, 380.0),
            t.muted,
            esc(l.race_note)
        ),
        format!(
            "<text x=\"28\" y=\"70\"{} fill=\"{}\">{}</text>",
            mono(12.5),
            t.muted,
            esc(
                "> Implement an order state machine: created, paid, shipped, delivered, cancelled, refunded…"
            )
        ),
    ];
    let t_end = |count: f64| t0 + (count / max) * run;
    for (i, (name, count)) in arms.into_iter().enumerate() {
        let y = 98.0 + i as f64 * 44.0;
        let bw = r1((count / max) * maxw);
        let me = i == 0;
        let te = t_end(count);
        o.push(format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"{}\" fill=\"{}\" text-anchor=\"end\">{}</text>",
            n(x - 16.0),
            n(y + 15.0),
            fit(name, 14.0, 118.0),
            if me { 700 } else { 500 },
            if me { t.accent } else { t.soft },
            esc(name)
        ));
        o.push(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"20\" rx=\"5\" fill=\"{}\">{}</rect>",
            n(x),
            n(y),
            n(bw),
            if me { t.fill } else { t.bar },
            anim_n(
                "width",
                &[(t0, 0.0), (te, bw), (d - 0.5, bw), (d - 0.15, 0.0)],
                d,
                PLAIN
            )
        ));
        o.push(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"2.5\" height=\"24\" rx=\"1\" fill=\"{}\" opacity=\"0\">{}{}</rect>",
            n(x),
            n(y - 2.0),
            t.text,
            anim_n("x", &[(t0, x), (te, r1(x + bw))], d, PLAIN),
            anim_n("opacity", &[(t0 - 0.01, 0.0), (t0, 1.0), (te, 1.0), (te + 0.01, 0.0)], d, PLAIN)
        ));
        o.push(format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"14\">{}<tspan font-weight=\"700\" fill=\"{}\">{}</tspan><tspan fill=\"{}\">{}</tspan></text>",
            n(r1(x + bw + 12.0)),
            n(y + 15.0),
            shown(te, d - 0.3, d, 0.2),
            if me { t.accent } else { t.text },
            grouped(count),
            t.muted,
            esc(l.tokens)
        ));
    }
    let cut = (l.cut)(js::round(100.0 * (1.0 - arms[0].1 / max)));
    let px = r1(x + (arms[0].1 / max) * maxw + 106.0);
    o.push(format!(
        "<g>{}<rect x=\"{}\" y=\"97\" width=\"{}\" height=\"22\" rx=\"11\" fill=\"{f}\" fill-opacity=\"0.12\" stroke=\"{f}\" stroke-opacity=\"0.45\"/>\
         <text x=\"{}\" y=\"112.5\" font-size=\"12.5\" font-weight=\"700\" fill=\"{}\">{}</text></g>",
        shown(t_end(max) + 0.2, d - 0.3, d, 0.25),
        n(px),
        n(r1(text_w(&cut, 12.5, false) * 1.12 + 24.0)),
        n(px + 12.0),
        t.accent,
        esc(&cut),
        f = t.fill,
    ));
    o.push(format!(
        "<text x=\"28\" y=\"{}\"{} fill=\"{}\">{}</text>",
        n(h - 22.0),
        mono(fit_as(l.race_foot, 11.5, 824.0, true)),
        t.faint,
        esc(l.race_foot)
    ));
    svg(
        t,
        w,
        h,
        "Same prompt, four rulesets, visible answer tokens: Chisle 567, ponytail 740, caveman 1,018, no ruleset 1,571. Chisle is 64% shorter than no ruleset.",
        &o,
        "",
    )
}

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

#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
pub(super) fn trim(t: &Theme, l: &Lang) -> String {
    let (w, h, d) = (880.0, 396.0, T_D);
    let right: [(usize, Vec<(&str, &str)>); 10] = [
        (0, vec![("> chisle@3.12.1 test", t.soft)]),
        (1, vec![("✓", t.green), (" retry backs off", t.soft)]),
        (6, vec![("... [chisle: line repeated 6×] ...", t.accent)]),
        (7, vec![("✓", t.green), (" strips colour codes", t.soft)]),
        (8, vec![("✓", t.green), (" collapses repeats", t.soft)]),
        (9, vec![("... [chisle: elided 172 lines,", t.accent)]),
        (9, vec![("     full output saved to a file] ...", t.accent)]),
        (
            10,
            vec![("✗ spill file is owner-only", t.red), (l.kept, t.green)],
        ),
        (11, vec![("  AssertionError: 0o644 !== 0o600", t.red)]),
        (12, vec![("ℹ tests 246  pass 245  fail 1", t.soft)]),
    ];
    let mut o = vec![
        win(t, 16.0, 16.0, 416.0, 330.0, "bash · npm test"),
        win(t, 448.0, 16.0, 416.0, 330.0, l.reads),
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
        ),
        format!("<g{}>", mono(12.5)),
    ];
    for (i, segs) in trim_left(t).iter().enumerate() {
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
                    anim_n("fill-opacity", &[(at, 1.0), (at + 0.3, end)], d, PLAIN)
                };
                format!(
                    "<tspan fill=\"{c}\" fill-opacity=\"{}\">{}{fade}</tspan>",
                    n(end),
                    esc(s)
                )
            })
            .collect::<Vec<_>>()
            .concat();
        o.push(format!(
            "<text x=\"28\" y=\"{}\">{spans}</text>",
            n(r1(T_Y0 + i as f64 * T_STEP))
        ));
    }
    for (row, (li, segs)) in right.iter().enumerate() {
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
    o.push(format!(
        "<text x=\"{}\" y=\"376\" font-size=\"{}\" fill=\"{}\" text-anchor=\"middle\" opacity=\"0\">{}{}</text>",
        n(w / 2.0),
        fit(l.foot1, 15.5, 820.0),
        t.muted,
        esc(l.foot1),
        anim_n("opacity", &[(0.0, 1.0), (6.0, 1.0), (6.2, 0.0), (d - 0.3, 0.0), (d - 0.1, 1.0)], d, PLAIN)
    ));
    o.push(format!(
        "<text x=\"{}\" y=\"376\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\" text-anchor=\"middle\">{}{}</text>",
        n(w / 2.0),
        fit(l.foot2, 15.5, 760.0),
        t.accent,
        esc(l.foot2),
        anim_n("opacity", &[(6.1, 0.0), (6.4, 1.0), (d - 0.5, 1.0), (d - 0.2, 0.0)], d, PLAIN)
    ));
    svg(
        t,
        w,
        h,
        "Animated illustration of the Chisle tool-output compressor: raw npm test output with colour codes, six identical lines and 172 passing lines becomes a short version with the codes stripped, the repeats collapsed, the middle elided with the full output saved to a file, and the failing test and its error kept. On real sessions, oversized output shrinks by about 48 percent on average.",
        &o,
        "",
    )
}

// ── Filters: command-aware filters on four real outputs ──────────────────────
// Chisle 3.14 on real output, from benchmarks/replay-commands.js (the failing
// jest run goes through the chisle-run.js wrapper, as on Claude Code in
// bypassPermissions mode). The footnote is that script's 37-command total.
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
pub(super) fn filters(t: &Theme, l: &Lang) -> String {
    let (w, h, d, x, maxw) = (880.0, 340.0, 9.0, 330.0, 420.0);
    let rows = [
        ("npm test", 19374.0, 445.0),
        ("npm test  # 1 failing", 16107.0, 309.0),
        ("git status", 485.0, 92.0),
        ("git log -n 15", 6561.0, 3784.0),
    ];
    let mut o = vec![
        format!(
            "<text x=\"28\" y=\"42\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\">{}</text>",
            fit(l.filt_title, 19.0, 470.0),
            t.text,
            esc(l.filt_title)
        ),
        format!(
            "<text x=\"{}\" y=\"42\" font-size=\"{}\" fill=\"{}\" text-anchor=\"end\">{}</text>",
            n(w - 28.0),
            fit(l.filt_note, 13.0, 330.0),
            t.muted,
            esc(l.filt_note)
        ),
        format!(
            "<text x=\"28\" y=\"68\" font-size=\"{}\" fill=\"{}\">{}</text>",
            fit(l.filt_sub, 13.5, 824.0),
            t.muted,
            esc(l.filt_sub)
        ),
    ];
    for (i, (cmd, raw, kept)) in rows.into_iter().enumerate() {
        let y = 96.0 + i as f64 * 50.0;
        let bw = f64::max(3.0, r1((kept / raw) * maxw));
        let t0 = 0.8 + i as f64 * 0.45;
        let t1 = t0 + 1.0;
        let pct = format!("−{}%", n(js::round(100.0 * (1.0 - kept / raw))));
        o.extend([
            format!("<text x=\"28\" y=\"{}\"{} fill=\"{}\">{}</text>", n(y + 15.0), mono(14.0), t.accent, esc(&format!("$ {cmd}"))),
            format!(
                "<text x=\"28\" y=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
                n(y + 34.0),
                fit(l.filt_notes[i], 12.0, 280.0),
                t.muted,
                esc(l.filt_notes[i])
            ),
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"16\" rx=\"5\" fill=\"{}\" fill-opacity=\"0.35\"/>",
                n(x),
                n(y + 3.0),
                n(maxw),
                t.bar
            ),
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"16\" rx=\"5\" fill=\"{}\">{}</rect>",
                n(x),
                n(y + 3.0),
                n(bw),
                t.fill,
                anim_n("width", &[(t0, maxw), (t1, bw), (d - 0.5, bw), (d - 0.15, maxw)], d, EASE)
            ),
            format!(
                "<text x=\"{}\" y=\"{}\"{} fill=\"{}\">{}</text>",
                n(x),
                n(y + 36.0),
                mono(11.5),
                t.faint,
                esc(&format!("{} → {} {}", grouped(raw), grouped(kept), l.chars))
            ),
            format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"17\" font-weight=\"700\" fill=\"{}\" text-anchor=\"end\">{}{}</text>",
                n(w - 28.0),
                n(y + 17.0),
                t.accent,
                shown(t1, d - 0.3, d, 0.2),
                esc(&pct)
            ),
        ]);
    }
    o.push(format!(
        "<text x=\"28\" y=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
        n(h - 22.0),
        fit(l.filt_foot, 12.5, 824.0),
        t.faint,
        esc(l.filt_foot)
    ));
    svg(
        t,
        w,
        h,
        "Command-aware filters on real output, raw characters to what Claude reads: npm test 19,374 to 445 (98% less), a failing npm test 16,107 to 309 (98% less), git status 485 to 92 (81% less), git log -n 15 6,561 to 3,784 (42% less). Across 37 commands Claude reads 28% of the raw output with Chisle and 84% with RTK. Errors, failures and summaries always stay.",
        &o,
        "",
    )
}
