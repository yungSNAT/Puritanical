// SPDX-License-Identifier: Apache-2.0 AND MIT

//! The banner and the stats panel.

use super::{
    ADV, DISCRETE, EASE, Lang, PLAIN, TRANSLATE, TRANSLATE_EASE, Theme, anim, anim_n, enter, esc,
    fit, js, mono, n, r1, shown, strs, svg, text_w, win,
};

// ── Banner: a wordy answer is chiselled down to the line that matters ─────────
const VERBOSE: &str = "Great question! Deep-copying objects in JavaScript can be surprisingly tricky. You could write a recursive helper, or install lodash and use cloneDeep, but honestly the simplest modern approach? Use structuredClone(config): it is built in, so it needs no dependency. Hope this helps! Let me know if you have any other questions.";
// The kept words read as one plain sentence: the fact and its reason.
const KEEP: [&str; 6] = [
    "Use",
    "structuredClone(config):",
    "it",
    "needs",
    "no",
    "dependency.",
];

struct Placed<'a> {
    w: &'a str,
    line: usize,
    col: usize,
}

const B_X0: f64 = 616.0;
const B_Y0: f64 = 140.0;
const B_FS: f64 = 15.0;
const B_LH: f64 = 23.0;
const B_WRAP: usize = 56;
const B_D: f64 = 11.0;

fn b_cw() -> f64 {
    B_FS * ADV
}

fn t_line(line: usize) -> f64 {
    3.1 + line as f64 * 0.3
}

/// Wraps the verbose answer as the terminal prints it.
fn place(words: &[&'static str]) -> Vec<Placed<'static>> {
    let mut pos = Vec::new();
    let (mut line, mut col) = (0, 2);
    for &w in words {
        if col > 2 && col + 1 + js::len(w) > B_WRAP {
            line += 1;
            col = 2;
        } else if col > 2 {
            col += 1;
        }
        pos.push(Placed { w, line, col });
        col += js::len(w);
    }
    pos
}

pub(super) fn banner(t: &Theme, l: &Lang) -> String {
    let (w, h) = (1200.0, 420.0);
    let defs = format!(
        "<pattern id=\"dots\" width=\"22\" height=\"22\" patternUnits=\"userSpaceOnUse\"><circle cx=\"2\" cy=\"2\" r=\"1.3\" fill=\"{}\"/></pattern>\
         <radialGradient id=\"glow\" cx=\"0.8\" cy=\"0.25\" r=\"0.65\"><stop offset=\"0\" stop-color=\"{f}\" stop-opacity=\"{}\"/><stop offset=\"1\" stop-color=\"{f}\" stop-opacity=\"0\"/></radialGradient>\
         <filter id=\"soft\" x=\"-3\" y=\"-1\" width=\"7\" height=\"3\"><feGaussianBlur stdDeviation=\"3\"/></filter>",
        t.dot,
        n(t.glow),
        f = t.fill
    );
    let mut o = vec![
        format!(
            "<rect x=\"1\" y=\"1\" width=\"{}\" height=\"{}\" rx=\"14\" fill=\"url(#dots)\"/>",
            n(w - 2.0),
            n(h - 2.0)
        ),
        format!(
            "<rect x=\"1\" y=\"1\" width=\"{}\" height=\"{}\" rx=\"14\" fill=\"url(#glow)\"/>",
            n(w - 2.0),
            n(h - 2.0)
        ),
    ];
    banner_pitch(&mut o, t, l);
    let words: Vec<&str> = VERBOSE.split(' ').collect();
    banner_terminal(&mut o, t, l, &words);
    let label = format!(
        "Puritanical: plain sentences, answer first. Animated: a {}-word AI answer loses its opener and closer and becomes one plain sentence, \"Use structuredClone(config): it needs no dependency.\"",
        words.len()
    );
    svg(t, w, h, &label, &o, &defs)
}

/// Left: the pitch.
fn banner_pitch(o: &mut Vec<String>, t: &Theme, l: &Lang) {
    o.extend([
        format!(
            "<rect x=\"64\" y=\"58\" width=\"{}\" height=\"30\" rx=\"15\" fill=\"{}\" stroke=\"{}\"/>",
            n(r1(text_w(l.pill, 13.0, true) + 44.0)),
            t.card,
            t.border
        ),
        format!(
            "<circle cx=\"81\" cy=\"73\" r=\"4\" fill=\"{}\">{}</circle>",
            t.green,
            anim_n("opacity", &[(0.0, 1.0), (1.0, 0.3), (2.0, 1.0)], 2.0, PLAIN)
        ),
        format!("<text x=\"93\" y=\"77.5\"{} fill=\"{}\">{}</text>", mono(13.0), t.muted, esc(l.pill)),
        format!(
            "<text x=\"58\" y=\"196\"{} font-weight=\"800\" letter-spacing=\"-2\" fill=\"{}\">puritanical</text>",
            mono(96.0),
            t.text
        ),
        format!(
            "<rect x=\"408\" y=\"126\" width=\"30\" height=\"80\" rx=\"3\" fill=\"{}\">{}</rect>",
            t.fill,
            anim_n("opacity", &[(0.0, 1.0), (0.55, 0.0)], 1.1, DISCRETE)
        ),
        format!(
            "<text x=\"64\" y=\"262\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>",
            fit(l.tag1, 26.0, 520.0),
            t.text,
            esc(l.tag1)
        ),
        format!(
            "<text x=\"64\" y=\"300\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\">{}</text>",
            fit(l.tag2, 26.0, 520.0),
            t.accent,
            esc(l.tag2)
        ),
        format!("<rect x=\"64\" y=\"330\" width=\"176\" height=\"44\" rx=\"10\" fill=\"{}\" stroke=\"{}\"/>", t.card, t.border),
        format!(
            "<text x=\"84\" y=\"358.5\"{}><tspan fill=\"{}\">$</tspan><tspan fill=\"{}\"> npx github:yungSNAT/Puritanical</tspan></text>",
            mono(18.0),
            t.faint,
            t.text
        ),
    ]);
}

/// Right: the terminal, the falling filler, the chisel and the receipt.
fn banner_terminal(o: &mut Vec<String>, t: &Theme, l: &Lang, words: &[&'static str]) {
    o.extend(banner_frame(t));
    let pos = place(words);
    let n_lines = pos.last().map_or(0, |p| p.line) + 1;
    let t_out = B_D - 1.15;
    o.push(format!(
        "<text x=\"{}\" y=\"{}\"{} fill=\"{}\">●{}</text>",
        n(B_X0),
        n(B_Y0),
        mono(B_FS),
        t.text,
        shown(0.2, t_out + 0.35, B_D, 0.15)
    ));
    banner_words(o, t, words, &pos, n_lines);
    banner_chisel(o, t, &pos, n_lines);
    o.push(banner_receipt(t, l, words.len(), n_lines));
}

/// The terminal window, the prompt and the blinking input box.
fn banner_frame(t: &Theme) -> [String; 5] {
    let cw = b_cw();
    [
        win(t, 596.0, 40.0, 564.0, 340.0, "claude  ~/app"),
        format!(
            "<text x=\"{}\" y=\"104\"{}><tspan fill=\"{}\">&gt;</tspan><tspan fill=\"{}\"> deep-copy this config object</tspan></text>",
            n(B_X0),
            mono(B_FS),
            t.accent,
            t.soft
        ),
        format!(
            "<rect x=\"612\" y=\"314\" width=\"532\" height=\"46\" rx=\"8\" fill=\"none\" stroke=\"{}\"/>",
            t.border
        ),
        format!(
            "<text x=\"628\" y=\"342\"{} fill=\"{}\">&gt;</text>",
            mono(B_FS),
            t.faint
        ),
        format!(
            "<rect x=\"{}\" y=\"327\" width=\"{}\" height=\"20\" fill=\"{}\">{}</rect>",
            n(r1(628.0 + 2.0 * cw)),
            n(r1(cw)),
            t.soft,
            anim_n("opacity", &[(0.0, 0.8), (0.55, 0.0)], 1.1, DISCRETE)
        ),
    ]
}

/// The receipt: words before and after, and the cut.
fn banner_receipt(t: &Theme, l: &Lang, total: usize, n_lines: usize) -> String {
    let t_out = B_D - 1.15;
    let stat = (l.words)(total, KEEP.len());
    let pct = format!(
        "−{}%",
        n(js::round(100.0 * (1.0 - KEEP.len() as f64 / total as f64)))
    );
    let chip = text_w(&format!("{stat} {pct}"), 13.0, true) + 28.0;
    let t_chip = t_line(n_lines) + 0.3 + 0.95;
    format!(
        "<g>{}<rect x=\"{x}\" y=\"162\" width=\"{}\" height=\"28\" rx=\"14\" fill=\"{f}\" fill-opacity=\"0.12\" stroke=\"{f}\" stroke-opacity=\"0.45\"/>\
         <text x=\"{}\" y=\"180.5\"{m}><tspan fill=\"{}\">{} </tspan><tspan fill=\"{}\" font-weight=\"700\">{pct}</tspan></text>\
         <text x=\"{}\" y=\"180.5\"{m} fill=\"{}\">{}</text></g>",
        shown(t_chip, t_out + 0.35, B_D, 0.25),
        n(r1(chip)),
        n(B_X0 + 14.0),
        t.muted,
        esc(&stat),
        t.accent,
        n(r1(B_X0 + chip + 14.0)),
        t.green,
        esc(l.same),
        x = n(B_X0),
        f = t.fill,
        m = mono(13.0),
    )
}

fn banner_words(o: &mut Vec<String>, t: &Theme, words: &[&str], pos: &[Placed], n_lines: usize) {
    let cw = b_cw();
    let keep_at: Vec<usize> = KEEP
        .iter()
        .filter_map(|k| words.iter().rposition(|w| w == k))
        .collect();
    let t_slide = t_line(n_lines) + 0.3;
    let mut fcol = 2;
    for (i, p) in pos.iter().enumerate() {
        let x = r1(B_X0 + p.col as f64 * cw);
        let y = B_Y0 + p.line as f64 * B_LH;
        let ts = 0.35 + i as f64 * 0.03;
        let th = t_line(p.line) + (p.col as f64 / B_WRAP as f64) * 0.3;
        if keep_at.contains(&i) {
            let fx = r1(B_X0 + fcol as f64 * cw);
            fcol += js::len(p.w) + 1;
            o.push(kept_word(t, p, [x, y, fx], [ts, th, t_slide]));
        } else {
            o.push(cut_word(t, p, [x, y], [ts, th]));
        }
    }
}

/// A kept word: it lights up, then slides from (x, y) into the answer at fx.
fn kept_word(t: &Theme, p: &Placed, xy: [f64; 3], times: [f64; 3]) -> String {
    let ([x, y, fx], [ts, th, t_slide]) = (xy, times);
    let t_out = B_D - 1.15;
    format!(
        "<text x=\"{}\" y=\"{}\"{} fill=\"{}\">{}{}{}{}{}</text>",
        n(fx),
        n(B_Y0),
        mono(B_FS),
        t.accent,
        esc(p.w),
        anim_n(
            "opacity",
            &[
                (ts, 0.0),
                (ts + 0.12, 1.0),
                (t_out, 1.0),
                (t_out + 0.35, 0.0)
            ],
            B_D,
            PLAIN
        ),
        anim(
            "fill",
            strs(&[(th, t.soft), (th + 0.15, t.accent)]),
            B_D,
            PLAIN
        ),
        anim_n("x", &[(t_slide, x), (t_slide + 0.75, fx)], B_D, EASE),
        anim_n("y", &[(t_slide, y), (t_slide + 0.75, B_Y0)], B_D, EASE),
    )
}

/// A cut word: it fades and drops away as the chisel passes.
fn cut_word(t: &Theme, p: &Placed, xy: [f64; 2], times: [f64; 2]) -> String {
    let ([x, y], [ts, th]) = (xy, times);
    format!(
        "<text x=\"{}\" y=\"{}\"{} fill=\"{}\" opacity=\"0\">{}{}{}</text>",
        n(x),
        n(y),
        mono(B_FS),
        t.soft,
        esc(p.w),
        anim_n(
            "opacity",
            &[(ts, 0.0), (ts + 0.12, 1.0), (th, 1.0), (th + 0.35, 0.0)],
            B_D,
            PLAIN
        ),
        anim(
            "transform",
            strs(&[(th, "0 0"), (th + 0.35, "0 16")]),
            B_D,
            TRANSLATE_EASE
        ),
    )
}

fn banner_chisel(o: &mut Vec<String>, t: &Theme, pos: &[Placed], n_lines: usize) {
    let cw = b_cw();
    let mut sweep = Vec::new();
    for line in 0..n_lines {
        let end = pos
            .iter()
            .filter(|p| p.line == line)
            .map(|p| p.col + js::len(p.w))
            .max()
            .unwrap_or_default();
        let y = n(B_Y0 + line as f64 * B_LH - 16.0);
        sweep.push((t_line(line), format!("{} {y}", n(r1(B_X0 + 2.0 * cw)))));
        sweep.push((
            t_line(line) + 0.3,
            format!("{} {y}", n(r1(B_X0 + end as f64 * cw))),
        ));
    }
    o.push(format!(
        "<g opacity=\"0\"><rect x=\"-6\" y=\"0\" width=\"12\" height=\"21\" rx=\"6\" fill=\"{}\" opacity=\"0.55\" filter=\"url(#soft)\"/>\
         <rect x=\"-1.5\" y=\"0\" width=\"3\" height=\"21\" rx=\"1.5\" fill=\"{}\"/>{}{}</g>",
        t.fill,
        t.accent,
        anim("transform", sweep, B_D, TRANSLATE),
        anim_n(
            "opacity",
            &[(t_line(0) - 0.01, 0.0), (t_line(0), 1.0), (t_line(n_lines) - 0.01, 1.0), (t_line(n_lines), 0.0)],
            B_D,
            PLAIN
        ),
    ));
}

// ── Stats: the four headline numbers ──────────────────────────────────────────
pub(super) fn stats(t: &Theme, l: &Lang) -> String {
    let (w, h) = (880.0, 248.0);
    let tiles = [
        ("−47%", l.tiles[0], 0.53),
        ("−51%", l.tiles[1], 0.49),
        ("−48%", l.tiles[2], 0.52),
        ("156/156", l.tiles[3], 1.0),
    ];
    let mut o = vec![format!(
        "<text x=\"{}\" y=\"34\" font-size=\"14\" fill=\"{}\" text-anchor=\"middle\">{}</text>",
        n(w / 2.0),
        t.muted,
        esc(l.stats_title)
    )];
    for (i, tile) in tiles.into_iter().enumerate() {
        o.push(stat_tile(t, i, tile));
    }
    svg(
        t,
        w,
        h,
        "Chisle versus the same model with no ruleset: on Sonnet 5.5, 47% fewer billed output tokens and answers 51% shorter; oversized tool output 48% smaller; accuracy 156 of 156, the same as the bare model.",
        &o,
        "",
    )
}

/// Tile `i`: the figure, its label and note, and a bar that shrinks to the figure.
fn stat_tile(t: &Theme, i: usize, tile: (&str, [&str; 2], f64)) -> String {
    let (v, [label, sub], p) = tile;
    let w = 880.0;
    let (tw, gap, y0) = (196.0, 16.0, 52.0);
    let x0 = (w - 4.0 * tw - 3.0 * gap) / 2.0;
    let bw = tw - 36.0;
    let x = x0 + i as f64 * (tw + gap);
    let full = i == 3;
    let c = if full { t.green } else { t.accent };
    let fc = if full { t.green } else { t.fill };
    let fw = r1(bw * p);
    let grow = if full {
        String::new()
    } else {
        anim_n("width", &[(0.0, bw), (1.0, bw), (2.5, fw)], 5.0, EASE)
    };
    format!(
        "<g>{}<rect x=\"{x}\" y=\"{}\" width=\"{}\" height=\"172\" rx=\"10\" fill=\"{}\" stroke=\"{}\"/>\
         <text x=\"{x18}\" y=\"{}\" font-size=\"42\" font-weight=\"800\" letter-spacing=\"-1\" fill=\"{c}\">{v}</text>\
         <text x=\"{x18}\" y=\"{}\" font-size=\"{}\" font-weight=\"600\" fill=\"{}\">{}</text>\
         <text x=\"{x18}\" y=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>\
         <rect x=\"{x18}\" y=\"{bar}\" width=\"{}\" height=\"8\" rx=\"4\" fill=\"{}\"/>\
         <rect x=\"{x18}\" y=\"{bar}\" width=\"{}\" height=\"8\" rx=\"4\" fill=\"{fc}\">{grow}</rect></g>",
        enter(0.15 * i as f64),
        n(y0),
        n(tw),
        t.card,
        t.border,
        n(y0 + 60.0),
        n(y0 + 92.0),
        fit(label, 15.0, 162.0),
        t.text,
        esc(label),
        n(y0 + 114.0),
        fit(sub, 12.5, 162.0),
        t.muted,
        esc(sub),
        n(bw),
        t.track,
        n(fw),
        x = n(x),
        x18 = n(x + 18.0),
        bar = n(y0 + 138.0),
    )
}
