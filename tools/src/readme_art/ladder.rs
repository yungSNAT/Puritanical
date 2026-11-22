// SPDX-License-Identifier: Apache-2.0 AND MIT

//! The ladder, install and commands panels.

use super::{
    ADV, EASE, Lang, PLAIN, Theme, anim_n, esc, fit, fit_as, js, mono, n, r1, shown, svg, tspans,
    win,
};

// ── Ladder: the efficiency ladder, walked for three requests ──────────────────
const L_SL: f64 = 4.5;
const L_Y0: f64 = 84.0;
const L_RH: f64 = 36.0;

fn row_y(r: usize) -> f64 {
    L_Y0 - 24.0 + r as f64 * L_RH
}

/// The three requests: what was asked, the rung it stops at, the answer.
type Scen = [(&'static str, usize, &'static str); 3];
/// One pass over the three requests.
const L_D: f64 = L_SL * 3.0;
const L_W: f64 = 880.0;

pub(super) fn ladder(t: &Theme, l: &Lang) -> String {
    let scen: Scen = [
        (l.reqs[0], 2, l.answers[0]),
        (l.reqs[1], 3, l.answers[1]),
        (l.reqs[2], 1, l.answers[2]),
    ];
    let mut o = vec![
        format!(
            "<text x=\"28\" y=\"40\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\">{}</text>",
            fit(l.ladder_title, 16.0, 820.0),
            t.text,
            esc(l.ladder_title)
        ),
        format!(
            "<path d=\"M46 {}V{}\" stroke=\"{}\" stroke-width=\"2\"/>",
            n(L_Y0 - 6.0),
            n(L_Y0 - 6.0 + 6.0 * L_RH),
            t.border
        ),
    ];
    o.extend(ladder_marker(t, &scen));
    for (r, rung) in l.rungs.iter().enumerate() {
        o.extend(ladder_rung(t, r, rung));
    }
    o.extend(ladder_requests(t, l, &scen));
    svg(
        t,
        L_W,
        396.0,
        "The Puritanical efficiency ladder. Before writing code, stop at the first rung that holds: 1 does this need to exist at all, 2 already in this codebase, 3 does the stdlib do it, 4 a native platform feature, 5 an already-installed dependency, 6 can it be one line, 7 only then the minimum code that works. Animated examples: deep-copying a config object stops at the stdlib with structuredClone; a date picker stops at the platform with input type date; formatting a date stops at reuse.",
        &o,
        "",
    )
}

/// The highlight and ring that climb to each request's rung.
fn ladder_marker(t: &Theme, scen: &Scen) -> [String; 2] {
    let mut hl = Vec::new();
    for (i, &(_, tgt, _)) in scen.iter().enumerate() {
        let s = i as f64 * L_SL;
        hl.push((s + 0.02, row_y(0)));
        for r in 1..=tgt {
            hl.push((s + 0.15 + r as f64 * 0.45, row_y(r - 1)));
            hl.push((s + 0.45 + r as f64 * 0.45, row_y(r)));
        }
        hl.push((s + L_SL - 0.05, row_y(tgt)));
    }
    let rest = row_y(scen[scen.len() - 1].1); // static frame: the last request's rung
    let circle: Vec<(f64, f64)> = hl.iter().map(|&(s, y)| (s, y + 16.0)).collect();
    [
        format!(
            "<rect x=\"20\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"{f}\" fill-opacity=\"0.13\" stroke=\"{f}\" stroke-opacity=\"0.55\">{}</rect>",
            n(rest),
            n(L_W - 40.0),
            n(L_RH - 4.0),
            anim_n("y", &hl, L_D, EASE),
            f = t.fill
        ),
        format!(
            "<circle cx=\"46\" cy=\"{}\" r=\"15\" fill=\"none\" stroke=\"{}\" stroke-width=\"2\">{}</circle>",
            n(rest + 16.0),
            t.fill,
            anim_n("cy", &circle, L_D, EASE)
        ),
    ]
}

/// Rung `r`: its number, the question and the answer that stops there.
fn ladder_rung(t: &Theme, r: usize, [q, a]: &[&str; 2]) -> [String; 3] {
    let y = L_Y0 + r as f64 * L_RH;
    [
        format!(
            "<circle cx=\"46\" cy=\"{}\" r=\"12\" fill=\"{}\" stroke=\"{}\"/><text x=\"46\" y=\"{}\" font-size=\"12.5\" font-weight=\"700\" fill=\"{}\" text-anchor=\"middle\">{}</text>",
            n(y - 6.0),
            t.card,
            t.border,
            n(y - 1.5),
            t.accent,
            r + 1
        ),
        format!(
            "<text x=\"72\" y=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>",
            n(y),
            fit(q, 14.5, 340.0),
            t.text,
            esc(q)
        ),
        format!(
            "<text x=\"420\" y=\"{}\" font-size=\"{}\" fill=\"{}\">→ {}</text>",
            n(y),
            fit(&format!("→ {a}"), 13.5, 432.0),
            t.muted,
            esc(a)
        ),
    ]
}

/// The footer: each request and the rung that answered it, in turn.
fn ladder_requests(t: &Theme, l: &Lang, scen: &Scen) -> Vec<String> {
    let yb = L_Y0 + 7.0 * L_RH + 8.0;
    let mut o = vec![format!(
        "<path d=\"M20 {}H{}\" stroke=\"{}\"/>",
        n(yb - 18.0),
        n(L_W - 20.0),
        t.border
    )];
    for (i, &(req, tgt, ans)) in scen.iter().enumerate() {
        let s = i as f64 * L_SL;
        let hit = s + 0.45 + tgt as f64 * 0.45 + 0.25;
        let opacity = u8::from(i == scen.len() - 1);
        let said = (l.hit)(tgt + 1, ans);
        o.push(format!(
            "<text x=\"28\" y=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{opacity}\">{} <tspan fill=\"{}\" font-weight=\"600\">{}</tspan>{}</text>",
            n(yb + 12.0),
            fit(&format!("{} {req}", l.request), 14.0, 470.0),
            t.muted,
            esc(l.request),
            t.text,
            esc(req),
            shown(s + 0.01, s + L_SL, L_D, 0.2)
        ));
        o.push(format!(
            "<text x=\"{}\" y=\"{}\"{} text-anchor=\"end\" fill=\"{}\" opacity=\"{opacity}\">{}{}</text>",
            n(L_W - 28.0),
            n(yb + 12.0),
            mono(fit_as(&said, 14.0, 360.0, true)),
            t.green,
            esc(&said),
            shown(hit, s + L_SL, L_D, 0.2)
        ));
    }
    o
}

/// Where and when a typed command appears.
struct Typing<'a> {
    id: &'a str,
    x: f64,
    y: f64,
    fs: f64,
    t0: f64,
    t1: f64,
    d: f64,
}

/// Types `segs` at (x, y) between t0 and t1 behind a caret.
fn typed(t: &Theme, at: &Typing, segs: &[(&str, &str)]) -> String {
    let &Typing {
        id,
        x,
        y,
        fs,
        t0,
        t1,
        d,
    } = at;
    let chars = segs.iter().map(|(s, _)| js::len(s)).sum::<usize>() as f64;
    let w = r1(chars * fs * ADV);
    format!(
        "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">{}</rect></clipPath>\
         <text x=\"{}\" y=\"{}\"{} clip-path=\"url(#{id})\">{}</text>\
         <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" opacity=\"0\">{}{}</rect>",
        n(x),
        n(y - fs),
        n(w),
        n(r1(fs * 1.5)),
        anim_n("width", &[(t0, 0.0), (t1, w)], d, PLAIN),
        n(x),
        n(y),
        mono(fs),
        tspans(segs),
        n(x),
        n(y - fs + 1.0),
        n(r1(fs * ADV)),
        n(fs + 3.0),
        t.soft,
        anim_n("x", &[(t0, x), (t1, r1(x + w))], d, PLAIN),
        anim_n(
            "opacity",
            &[
                (t0 - 0.01, 0.0),
                (t0, 0.75),
                (t1 + 0.3, 0.75),
                (t1 + 0.31, 0.0)
            ],
            d,
            PLAIN
        )
    )
}

/// Lines that print after a command: each fades in at its time.
fn printed(x: f64, y: f64, segs: &[(&str, &str)], fs: f64, at: f64, d: f64) -> String {
    format!(
        "<text x=\"{}\" y=\"{}\"{} opacity=\"1\">{}{}</text>",
        n(x),
        n(y),
        mono(fs),
        tspans(segs),
        anim_n("opacity", &[(at, 0.0), (at + 0.15, 1.0)], d, PLAIN)
    )
}

// ── Install: what `npx github:yungSNAT/Puritanical` prints (format copied from bin/install.js) ────
const AGENTS: [&str; 6] = [
    "Claude Code",
    "Gemini CLI",
    "Codex",
    "OpenCode",
    "Copilot CLI",
    "Cursor",
];
const AGENT_IDS: [&str; 6] = [
    "claude",
    "gemini",
    "codex",
    "opencode",
    "copilot-cli",
    "cursor",
];
const I_FS: f64 = 13.5;
const I_X: f64 = 40.0;
const I_Y0: f64 = 82.0;
const I_D: f64 = 10.0;

pub(super) fn install(t: &Theme, _: &Lang) -> String {
    let out = I_D - 0.6;
    let cmd = Typing {
        id: "cmd",
        x: I_X,
        y: I_Y0,
        fs: I_FS,
        t0: 0.5,
        t1: 1.2,
        d: I_D,
    };
    let o = [
        win(
            t,
            16.0,
            16.0,
            848.0,
            388.0,
            "~/app — npx github:yungSNAT/Puritanical",
        ),
        format!(
            "<g>{}{}{}</g>",
            anim_n("opacity", &[(out, 1.0), (out + 0.3, 0.0)], I_D, PLAIN),
            typed(
                t,
                &cmd,
                &[("$ ", t.faint), ("npx github:yungSNAT/Puritanical", t.text)]
            ),
            install_output(t)
        ),
    ];
    let label = format!(
        "Animated terminal: npx github:yungSNAT/Puritanical detects {}, installs into each, and prints \"Done. Restart your agent.\"",
        AGENTS.join(", ")
    );
    svg(t, 880.0, 420.0, &label, &o, "")
}

/// The installer's report, a line at a time.
fn install_output(t: &Theme) -> String {
    let lh = 21.5;
    let detected: Vec<String> = AGENTS.iter().map(|a| format!("{a} detected")).collect();
    let count = format!("  detected:  {}", AGENTS.len());
    let installed = format!("  installed: {}", AGENT_IDS.join(", "));
    let mut lines: Vec<(f64, Vec<(&str, &str)>)> =
        vec![(1.7, vec![("╭─ Puritanical installer ─╮", t.accent)])];
    for (i, a) in detected.iter().enumerate() {
        lines.push((2.1 + i as f64 * 0.35, vec![("→ ", t.accent), (a, t.soft)]));
    }
    lines.extend([
        (4.3, vec![("── summary ──", t.accent)]),
        (4.45, vec![(count.as_str(), t.soft)]),
        (4.6, vec![(installed.as_str(), t.green)]),
        (
            5.0,
            vec![
                ("Done.", t.accent),
                (" Restart your agent. Type ", t.soft),
                ("/puritanical", t.accent),
                (" in Claude Code or Pi, or just start coding.", t.soft),
            ],
        ),
    ]);
    // Blank rows between blocks, as the installer prints them.
    let rows = [1.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 11.0, 12.0, 14.0];
    lines
        .iter()
        .zip(rows)
        .map(|((at, segs), row)| printed(I_X, r1(I_Y0 + row * lh), segs, I_FS, *at, I_D))
        .collect()
}

// ── Commands: a typed cheat sheet ─────────────────────────────────────────────
const COMMANDS: [(&str, &str, &str); 8] = [
    (
        "$",
        "npx github:yungSNAT/Puritanical",
        "wire every agent on this machine",
    ),
    (
        "$",
        "npx github:yungSNAT/Puritanical --dry-run",
        "preview first",
    ),
    (
        "$",
        "npx github:yungSNAT/Puritanical --uninstall",
        "put everything back",
    ),
    (">", "/puritanical off", "pause it"),
    (
        ">",
        "/puritanical-review",
        "over-engineering, one finding per line",
    ),
    (
        ">",
        "/puritanical-audit",
        "bloated code and prose, biggest cut first",
    ),
    (
        "$",
        "npx github:yungSNAT/Puritanical tools",
        "tool schemas you pay for but never use",
    ),
    (
        "$",
        "npx github:yungSNAT/Puritanical mcp wrap",
        "trim MCP output in any agent",
    ),
];
const C_D: f64 = 11.0;

pub(super) fn commands(t: &Theme, l: &Lang) -> String {
    let out = C_D - 0.6;
    let mut o = vec![
        win(t, 16.0, 16.0, 848.0, 280.0, l.sheet),
        format!(
            "<g>{}",
            anim_n("opacity", &[(out, 1.0), (out + 0.3, 0.0)], C_D, PLAIN)
        ),
    ];
    for (i, &(p, c, _)) in COMMANDS.iter().enumerate() {
        o.extend(command_row(t, i, (p, c), l.notes[i]));
    }
    o.push("</g>".into());
    let sheet: Vec<String> = COMMANDS
        .iter()
        .map(|(_, c, note)| format!("{c}: {note}"))
        .collect();
    svg(
        t,
        880.0,
        312.0,
        &format!("Animated cheat sheet: {}.", sheet.join("; ")),
        &o,
        "",
    )
}

/// Row `i`: the prompt and command typed out, then its note.
fn command_row(t: &Theme, i: usize, (p, c): (&str, &str), note: &str) -> [String; 2] {
    let (fs, x, y0, lh, pad) = (14.0, 40.0, 84.0, 27.0, 26.0);
    let y = y0 + i as f64 * lh;
    let t0 = 0.5 + i as f64 * 0.85;
    let t1 = t0 + 0.45;
    let id = format!("c{i}");
    let prompt = format!("{p} ");
    let colour = if p == "$" { t.faint } else { t.accent };
    let note = format!("# {note}");
    [
        typed(
            t,
            &Typing {
                id: &id,
                x,
                y,
                fs,
                t0,
                t1,
                d: C_D,
            },
            &[(&prompt, colour), (c, t.text)],
        ),
        printed(
            r1(x + pad * fs * ADV),
            y,
            &[(&note, t.muted)],
            fs,
            t1 + 0.15,
            C_D,
        ),
    ]
}
