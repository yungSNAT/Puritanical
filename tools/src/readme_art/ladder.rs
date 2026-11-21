// SPDX-License-Identifier: MIT

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

#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
pub(super) fn ladder(t: &Theme, l: &Lang) -> String {
    let scen = [
        (l.reqs[0], 2, l.answers[0]),
        (l.reqs[1], 3, l.answers[1]),
        (l.reqs[2], 1, l.answers[2]),
    ];
    let (w, h) = (880.0, 396.0);
    let d = L_SL * scen.len() as f64;
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
    o.push(format!(
        "<rect x=\"20\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"{f}\" fill-opacity=\"0.13\" stroke=\"{f}\" stroke-opacity=\"0.55\">{}</rect>",
        n(rest),
        n(w - 40.0),
        n(L_RH - 4.0),
        anim_n("y", &hl, d, EASE),
        f = t.fill
    ));
    o.push(format!(
        "<circle cx=\"46\" cy=\"{}\" r=\"15\" fill=\"none\" stroke=\"{}\" stroke-width=\"2\">{}</circle>",
        n(rest + 16.0),
        t.fill,
        anim_n("cy", &circle, d, EASE)
    ));
    for (r, [q, a]) in l.rungs.iter().enumerate() {
        let y = L_Y0 + r as f64 * L_RH;
        o.push(format!(
            "<circle cx=\"46\" cy=\"{}\" r=\"12\" fill=\"{}\" stroke=\"{}\"/><text x=\"46\" y=\"{}\" font-size=\"12.5\" font-weight=\"700\" fill=\"{}\" text-anchor=\"middle\">{}</text>",
            n(y - 6.0),
            t.card,
            t.border,
            n(y - 1.5),
            t.accent,
            r + 1
        ));
        o.push(format!(
            "<text x=\"72\" y=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>",
            n(y),
            fit(q, 14.5, 340.0),
            t.text,
            esc(q)
        ));
        o.push(format!(
            "<text x=\"420\" y=\"{}\" font-size=\"{}\" fill=\"{}\">→ {}</text>",
            n(y),
            fit(&format!("→ {a}"), 13.5, 432.0),
            t.muted,
            esc(a)
        ));
    }
    let yb = L_Y0 + 7.0 * L_RH + 8.0;
    o.push(format!(
        "<path d=\"M20 {}H{}\" stroke=\"{}\"/>",
        n(yb - 18.0),
        n(w - 20.0),
        t.border
    ));
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
            shown(s + 0.01, s + L_SL, d, 0.2)
        ));
        o.push(format!(
            "<text x=\"{}\" y=\"{}\"{} text-anchor=\"end\" fill=\"{}\" opacity=\"{opacity}\">{}{}</text>",
            n(w - 28.0),
            n(yb + 12.0),
            mono(fit_as(&said, 14.0, 360.0, true)),
            t.green,
            esc(&said),
            shown(hit, s + L_SL, d, 0.2)
        ));
    }
    svg(
        t,
        w,
        h,
        "The Chisle efficiency ladder. Before writing code, stop at the first rung that holds: 1 does this need to exist at all, 2 already in this codebase, 3 does the stdlib do it, 4 a native platform feature, 5 an already-installed dependency, 6 can it be one line, 7 only then the minimum code that works. Animated examples: deep-copying a config object stops at the stdlib with structuredClone; a date picker stops at the platform with input type date; formatting a date stops at reuse.",
        &o,
        "",
    )
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

// ── Install: what `npx chisle` prints (format copied from bin/install.js) ────
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
pub(super) fn install(t: &Theme, _: &Lang) -> String {
    let (w, h, d, fs, x, y0, lh) = (880.0, 420.0, 10.0, 13.5, 40.0, 82.0, 21.5);
    let out = d - 0.6;
    let agents = [
        "Claude Code",
        "Gemini CLI",
        "Codex",
        "OpenCode",
        "Copilot CLI",
        "Cursor",
    ];
    let ids = [
        "claude",
        "gemini",
        "codex",
        "opencode",
        "copilot-cli",
        "cursor",
    ];
    let detected: Vec<String> = agents.iter().map(|a| format!("{a} detected")).collect();
    let count = format!("  detected:  {}", agents.len());
    let installed = format!("  installed: {}", ids.join(", "));
    let mut lines: Vec<(f64, Vec<(&str, &str)>)> =
        vec![(1.7, vec![("╭─ Chisle installer ─╮", t.accent)])];
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
                ("/chisle", t.accent),
                (" in Claude Code or Pi, or just start coding.", t.soft),
            ],
        ),
    ]);
    // Blank rows between blocks, as the installer prints them.
    let rows = [1.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 11.0, 12.0, 14.0];
    let cmd = Typing {
        id: "cmd",
        x,
        y: y0,
        fs,
        t0: 0.5,
        t1: 1.2,
        d,
    };
    let printed_lines: String = lines
        .iter()
        .zip(rows)
        .map(|((at, segs), row)| printed(x, r1(y0 + row * lh), segs, fs, *at, d))
        .collect();
    let o = [
        win(t, 16.0, 16.0, 848.0, 388.0, "~/app — npx chisle"),
        format!(
            "<g>{}{}{printed_lines}</g>",
            anim_n("opacity", &[(out, 1.0), (out + 0.3, 0.0)], d, PLAIN),
            typed(t, &cmd, &[("$ ", t.faint), ("npx chisle", t.text)])
        ),
    ];
    let label = format!(
        "Animated terminal: npx chisle detects {}, installs into each, and prints \"Done. Restart your agent.\"",
        agents.join(", ")
    );
    svg(t, w, h, &label, &o, "")
}

// ── Commands: a typed cheat sheet ─────────────────────────────────────────────
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
pub(super) fn commands(t: &Theme, l: &Lang) -> String {
    let (w, h, d, fs, x, y0, lh, pad) = (880.0, 312.0, 11.0, 14.0, 40.0, 84.0, 27.0, 26.0);
    let cmds = [
        ("$", "npx chisle", "wire every agent on this machine"),
        ("$", "npx chisle --dry-run", "preview first"),
        ("$", "npx chisle --uninstall", "put everything back"),
        (">", "/chisle off", "pause it"),
        (
            ">",
            "/chisle-review",
            "over-engineering, one finding per line",
        ),
        (
            ">",
            "/chisle-audit",
            "bloated code and prose, biggest cut first",
        ),
        (
            "$",
            "npx chisle tools",
            "tool schemas you pay for but never use",
        ),
        ("$", "npx chisle mcp wrap", "trim MCP output in any agent"),
    ];
    let out = d - 0.6;
    let mut o = vec![
        win(t, 16.0, 16.0, 848.0, 280.0, l.sheet),
        format!(
            "<g>{}",
            anim_n("opacity", &[(out, 1.0), (out + 0.3, 0.0)], d, PLAIN)
        ),
    ];
    for (i, (p, c, _)) in cmds.iter().enumerate() {
        let y = y0 + i as f64 * lh;
        let t0 = 0.5 + i as f64 * 0.85;
        let t1 = t0 + 0.45;
        let id = format!("c{i}");
        let prompt = format!("{p} ");
        let colour = if *p == "$" { t.faint } else { t.accent };
        let note = format!("# {}", l.notes[i]);
        o.push(typed(
            t,
            &Typing {
                id: &id,
                x,
                y,
                fs,
                t0,
                t1,
                d,
            },
            &[(&prompt, colour), (c, t.text)],
        ));
        o.push(printed(
            r1(x + pad * fs * ADV),
            y,
            &[(&note, t.muted)],
            fs,
            t1 + 0.15,
            d,
        ));
    }
    o.push("</g>".into());
    let sheet: Vec<String> = cmds
        .iter()
        .map(|(_, c, note)| format!("{c}: {note}"))
        .collect();
    svg(
        t,
        w,
        h,
        &format!("Animated cheat sheet: {}.", sheet.join("; ")),
        &o,
        "",
    )
}
