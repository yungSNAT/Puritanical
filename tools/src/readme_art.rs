// SPDX-License-Identifier: MIT

//! The animated README art in `assets/readme/`: a -dark and a -light SVG per
//! piece (the README picks one with <picture>). Plain SVG + SMIL, so it
//! animates inside GitHub's <img> sandbox with no scripts and no web fonts.
//! Every static attribute holds the final frame, so a renderer without SMIL
//! shows the result.
//!
//! The figures mirror the README's benchmark sections; change them together.
//! Race tokens: `benchmarks/results/raw-314-sidebyside/large-s2/statemachine__*.json`
//! (`usage.output_tokens` minus `thinking_tokens`).

use crate::Out;
use crate::js::{self, grouped, num as n, to_fixed};
use std::fmt::Write as _;
use std::path::Path;

const SANS: &str =
    "-apple-system,BlinkMacSystemFont,'Segoe UI','Noto Sans',Helvetica,Arial,sans-serif";
const MONO: &str =
    "ui-monospace,SFMono-Regular,'SF Mono',Menlo,Consolas,'Liberation Mono',monospace";
/// Monospace advance per px of font size; the mono layouts assume it.
const ADV: f64 = 0.6;

struct Theme {
    bg: &'static str,
    card: &'static str,
    border: &'static str,
    text: &'static str,
    soft: &'static str,
    muted: &'static str,
    faint: &'static str,
    accent: &'static str,
    fill: &'static str,
    track: &'static str,
    dot: &'static str,
    bar: &'static str,
    green: &'static str,
    red: &'static str,
    glow: f64,
}

const THEMES: [(&str, Theme); 2] = [
    (
        "dark",
        Theme {
            bg: "#0d1117",
            card: "#161b22",
            border: "#30363d",
            text: "#e6edf3",
            soft: "#c9d1d9",
            muted: "#8b949e",
            faint: "#6e7681",
            accent: "#f0a85a",
            fill: "#d78a3c",
            track: "#21262d",
            dot: "#1f2630",
            bar: "#484f58",
            green: "#3fb950",
            red: "#f85149",
            glow: 0.18,
        },
    ),
    (
        "light",
        Theme {
            bg: "#ffffff",
            card: "#f6f8fa",
            border: "#d0d7de",
            text: "#1f2328",
            soft: "#31363c",
            muted: "#59636e",
            faint: "#818b98",
            accent: "#b35a14",
            fill: "#d78a3c",
            track: "#eaeef2",
            dot: "#e4e8ec",
            bar: "#afb8c1",
            green: "#1a7f37",
            red: "#cf222e",
            glow: 0.12,
        },
    ),
];

/// Text inside the art, per README language. Terminal commands, CLI output
/// and code stay English everywhere; only the prose around them is translated.
struct Lang {
    code: &'static str,
    pill: &'static str,
    tag1: &'static str,
    tag2: &'static str,
    words: fn(usize, usize) -> String,
    same: &'static str,
    stats_title: &'static str,
    tiles: [[&'static str; 2]; 4],
    race_title: &'static str,
    race_note: &'static str,
    bare: &'static str,
    tokens: &'static str,
    cut: fn(f64) -> String,
    race_foot: &'static str,
    reads: &'static str,
    kept: &'static str,
    foot1: &'static str,
    foot2: &'static str,
    ladder_title: &'static str,
    rungs: [[&'static str; 2]; 7],
    request: &'static str,
    reqs: [&'static str; 3],
    answers: [&'static str; 3],
    hit: fn(usize, &str) -> String,
    sheet: &'static str,
    notes: [&'static str; 8],
    filt_title: &'static str,
    filt_note: &'static str,
    filt_sub: &'static str,
    filt_notes: [&'static str; 4],
    filt_foot: &'static str,
    chars: &'static str,
    strip_title: &'static str,
    strip_sub: &'static str,
    free: &'static str,
    not_free: &'static str,
    body1: &'static str,
    body2: &'static str,
    button: &'static str,
}

// Rough rendered width: CJK and Hangul glyphs take about 1em; Latin about
// 0.5em in the sans stack and 0.6em in mono. fit() shrinks a font size until
// the text fits.
fn wide(c: char) -> bool {
    matches!(c, '\u{1100}'..='\u{11ff}' | '\u{2e80}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' | '\u{f900}'..='\u{faff}' | '\u{ff00}'..='\u{ffef}')
}

fn text_w(s: &str, fs: f64, mono: bool) -> f64 {
    s.chars().fold(0.0, |w, c| {
        w + fs
            * if wide(c) {
                1.0
            } else if mono {
                ADV
            } else {
                0.5
            }
    })
}

fn fit_as(s: &str, fs: f64, maxw: f64, mono: bool) -> f64 {
    fs.min(((fs * maxw) / text_w(s, fs, mono) * 10.0).floor() / 10.0)
}

fn fit(s: &str, fs: f64, maxw: f64) -> String {
    n(fit_as(s, fs, maxw, false))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn r1(x: f64) -> f64 {
    js::round(x * 10.0) / 10.0
}

fn mono(size: f64) -> String {
    format!(" font-family=\"{MONO}\" font-size=\"{}\"", n(size))
}

#[derive(Clone, Copy, Default)]
struct Opt {
    ease: bool,
    discrete: bool,
    kind: Option<&'static str>,
}

const PLAIN: Opt = Opt {
    ease: false,
    discrete: false,
    kind: None,
};
const EASE: Opt = Opt {
    ease: true,
    discrete: false,
    kind: None,
};
const DISCRETE: Opt = Opt {
    ease: false,
    discrete: true,
    kind: None,
};
const TRANSLATE: Opt = Opt {
    ease: false,
    discrete: false,
    kind: Some("translate"),
};
const TRANSLATE_EASE: Opt = Opt {
    ease: true,
    discrete: false,
    kind: Some("translate"),
};

fn nums(pts: &[(f64, f64)]) -> Vec<(f64, String)> {
    pts.iter().map(|&(t, v)| (t, n(v))).collect()
}

fn strs(pts: &[(f64, &str)]) -> Vec<(f64, String)> {
    pts.iter().map(|&(t, v)| (t, v.to_string())).collect()
}

/// One looping timeline. `pts` are (seconds, value) in time order; the first
/// and last values are held to the loop's ends.
fn anim(attr: &str, mut p: Vec<(f64, String)>, dur: f64, o: Opt) -> String {
    if p[0].0 > 0.0 {
        p.insert(0, (0.0, p[0].1.clone()));
    }
    let last = p[p.len() - 1].clone();
    if last.0 < dur {
        p.push((dur, last.1));
    }
    let tag = if o.kind.is_some() {
        "animateTransform"
    } else {
        "animate"
    };
    let kind = o.kind.map_or(String::new(), |k| format!(" type=\"{k}\""));
    let values: Vec<&str> = p.iter().map(|x| x.1.as_str()).collect();
    let times: Vec<String> = p.iter().map(|x| to_fixed(x.0 / dur, 4)).collect();
    let mut a = format!(
        "<{tag} attributeName=\"{attr}\"{kind} values=\"{}\" keyTimes=\"{}\" dur=\"{}s\" repeatCount=\"indefinite\"",
        values.join(";"),
        times.join(";"),
        n(dur)
    );
    if o.discrete {
        a += " calcMode=\"discrete\"";
    } else if o.ease {
        let _ = write!(
            a,
            " calcMode=\"spline\" keySplines=\"{}\"",
            vec!["0.4 0 0.2 1"; p.len() - 1].join(";")
        );
    }
    a + "/>"
}

fn anim_n(attr: &str, pts: &[(f64, f64)], dur: f64, o: Opt) -> String {
    anim(attr, nums(pts), dur, o)
}

/// Visible from t0 to t1 of each loop.
fn shown(t0: f64, t1: f64, dur: f64, f: f64) -> String {
    anim_n(
        "opacity",
        &[(t0, 0.0), (t0 + f, 1.0), (t1 - f, 1.0), (t1, 0.0)],
        dur,
        PLAIN,
    )
}

/// Fade and rise in once, `delay` seconds after load.
fn enter(delay: f64) -> String {
    let at = to_fixed(delay / (delay + 0.6), 3);
    let dur = n(delay + 0.6);
    format!(
        "<animate attributeName=\"opacity\" values=\"0;0;1\" keyTimes=\"0;{at};1\" dur=\"{dur}s\" fill=\"freeze\"/>\
         <animateTransform attributeName=\"transform\" type=\"translate\" values=\"0 10;0 10;0 0\" keyTimes=\"0;{at};1\" dur=\"{dur}s\" fill=\"freeze\" calcMode=\"spline\" keySplines=\"0 0 1 1;0.2 0.7 0.2 1\"/>"
    )
}

fn svg(t: &Theme, w: f64, h: f64, label: &str, body: &[String], defs: &str) -> String {
    let defs = if defs.is_empty() {
        String::new()
    } else {
        format!("<defs>{defs}</defs>\n")
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"{SANS}\" xml:space=\"preserve\" role=\"img\" aria-label=\"{}\">
{defs}<rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"14\" fill=\"{}\" stroke=\"{}\"/>
{}
</svg>
",
        esc(label),
        n(w - 1.0),
        n(h - 1.0),
        t.bg,
        t.border,
        body.join("\n"),
        w = n(w),
        h = n(h),
    )
}

/// A terminal window: card, title bar, traffic lights, centred title.
fn win(t: &Theme, x: f64, y: f64, w: f64, h: f64, title: &str) -> String {
    let lights = ["#ff5f57", "#febc2e", "#28c840"]
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"5\" fill=\"{c}\"/>",
                n(x + 18.0 + i as f64 * 16.0),
                n(y + 16.0)
            )
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"10\" fill=\"{}\" stroke=\"{}\"/><path d=\"M{} {}H{}\" stroke=\"{}\"/>{lights}<text x=\"{}\" y=\"{}\"{} fill=\"{}\" text-anchor=\"middle\">{}</text>",
        n(x),
        n(y),
        n(w),
        n(h),
        t.card,
        t.border,
        n(x),
        n(y + 32.0),
        n(x + w),
        t.border,
        n(x + w / 2.0),
        n(y + 20.5),
        mono(12.0),
        t.muted,
        esc(title)
    )
}

/// `[[text, colour], ...]` as tspans.
fn tspans(segs: &[(&str, &str)]) -> String {
    segs.iter()
        .map(|(s, c)| format!("<tspan fill=\"{c}\">{}</tspan>", esc(s)))
        .collect::<Vec<_>>()
        .concat()
}

// ── Banner: a wordy answer is chiselled down to the line that matters ─────────
const VERBOSE: &str = "Great question! Deep-copying objects in JavaScript can be surprisingly tricky. You could write a recursive helper, or install lodash and use cloneDeep, but honestly the simplest modern approach is structuredClone(config), which is built in, so there's no dependency to install. Hope this helps! Let me know if you have any other questions.";
const KEEP: [&str; 5] = [
    "structuredClone(config),",
    "built",
    "in,",
    "no",
    "dependency",
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

fn banner(t: &Theme, l: &Lang) -> String {
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
        "Chisle: fewer tokens in, fewer tokens out, same answers. Animated: a {}-word AI answer is chiselled down to \"structuredClone(config), built in, no dependency\".",
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
            "<text x=\"58\" y=\"196\"{} font-weight=\"800\" letter-spacing=\"-2\" fill=\"{}\">chisle</text>",
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
            "<text x=\"84\" y=\"358.5\"{}><tspan fill=\"{}\">$</tspan><tspan fill=\"{}\"> npx chisle</tspan></text>",
            mono(18.0),
            t.faint,
            t.text
        ),
    ]);
}

/// Right: the terminal, the falling filler, the chisel and the receipt.
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
fn banner_terminal(o: &mut Vec<String>, t: &Theme, l: &Lang, words: &[&'static str]) {
    let cw = b_cw();
    o.extend([
        win(t, 596.0, 40.0, 564.0, 340.0, "claude  ~/app"),
        format!(
            "<text x=\"{}\" y=\"104\"{}><tspan fill=\"{}\">&gt;</tspan><tspan fill=\"{}\"> deep-copy this config object</tspan></text>",
            n(B_X0),
            mono(B_FS),
            t.accent,
            t.soft
        ),
        format!("<rect x=\"612\" y=\"314\" width=\"532\" height=\"46\" rx=\"8\" fill=\"none\" stroke=\"{}\"/>", t.border),
        format!("<text x=\"628\" y=\"342\"{} fill=\"{}\">&gt;</text>", mono(B_FS), t.faint),
        format!(
            "<rect x=\"{}\" y=\"327\" width=\"{}\" height=\"20\" fill=\"{}\">{}</rect>",
            n(r1(628.0 + 2.0 * cw)),
            n(r1(cw)),
            t.soft,
            anim_n("opacity", &[(0.0, 0.8), (0.55, 0.0)], 1.1, DISCRETE)
        ),
    ]);
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
    // The receipt.
    let stat = (l.words)(words.len(), KEEP.len());
    let pct = format!(
        "−{}%",
        n(js::round(
            100.0 * (1.0 - KEEP.len() as f64 / words.len() as f64)
        ))
    );
    let chip = text_w(&format!("{stat} {pct}"), 13.0, true) + 28.0;
    let t_chip = t_line(n_lines) + 0.3 + 0.95;
    o.push(format!(
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
    ));
}

#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
fn banner_words(o: &mut Vec<String>, t: &Theme, words: &[&str], pos: &[Placed], n_lines: usize) {
    let cw = b_cw();
    let keep_at: Vec<usize> = KEEP
        .iter()
        .filter_map(|k| words.iter().rposition(|w| w == k))
        .collect();
    let t_slide = t_line(n_lines) + 0.3;
    let t_out = B_D - 1.15;
    let mut fcol = 2;
    for (i, p) in pos.iter().enumerate() {
        let x = r1(B_X0 + p.col as f64 * cw);
        let y = B_Y0 + p.line as f64 * B_LH;
        let ts = 0.35 + i as f64 * 0.03;
        let th = t_line(p.line) + (p.col as f64 / B_WRAP as f64) * 0.3;
        if keep_at.contains(&i) {
            let fx = r1(B_X0 + fcol as f64 * cw);
            fcol += js::len(p.w) + 1;
            o.push(format!(
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
            ));
        } else {
            o.push(format!(
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
            ));
        }
    }
}

/// The chisel: a glowing blade that sweeps each line as its filler falls away.
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
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
fn stats(t: &Theme, l: &Lang) -> String {
    let (w, h) = (880.0, 248.0);
    let tiles = [
        ("−31%", l.tiles[0], 0.69),
        ("−49%", l.tiles[1], 0.51),
        ("−48%", l.tiles[2], 0.52),
        ("156/156", l.tiles[3], 1.0),
    ];
    let mut o = vec![format!(
        "<text x=\"{}\" y=\"34\" font-size=\"14\" fill=\"{}\" text-anchor=\"middle\">{}</text>",
        n(w / 2.0),
        t.muted,
        esc(l.stats_title)
    )];
    let (tw, gap, y0) = (196.0, 16.0, 52.0);
    let x0 = (w - 4.0 * tw - 3.0 * gap) / 2.0;
    let bw = tw - 36.0;
    for (i, (v, [label, sub], p)) in tiles.into_iter().enumerate() {
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
        o.push(format!(
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
        ));
    }
    svg(
        t,
        w,
        h,
        "Chisle versus the same model with no ruleset: 31% fewer billed output tokens, answers 49% shorter, oversized tool output 48% smaller, accuracy 156 of 156, the same as the bare model.",
        &o,
        "",
    )
}

// ── Race: one prompt, four rulesets, tokens streamed at the same speed ────────
#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
fn race(t: &Theme, l: &Lang) -> String {
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
fn trim(t: &Theme, l: &Lang) -> String {
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
fn filters(t: &Theme, l: &Lang) -> String {
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

// ── Ladder: the efficiency ladder, walked for three requests ──────────────────
const L_SL: f64 = 4.5;
const L_Y0: f64 = 84.0;
const L_RH: f64 = 36.0;

fn row_y(r: usize) -> f64 {
    L_Y0 - 24.0 + r as f64 * L_RH
}

#[expect(clippy::too_many_lines, reason = "one SVG panel, drawn in order")]
fn ladder(t: &Theme, l: &Lang) -> String {
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
fn install(t: &Theme, _: &Lang) -> String {
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
fn commands(t: &Theme, l: &Lang) -> String {
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

// ── Support: the coffee ask, as a slim strip (top) and a full card (bottom) ──
const BMC: &str = "#FFDD00";

/// A mug with rising steam; (x, y) is the top-left of an 80×100 box at scale s.
fn cup(t: &Theme, x: f64, y: f64, s: f64) -> String {
    let steam = [20, 34, 48]
        .iter()
        .enumerate()
        .map(|(i, sx)| {
            let p = i as f64 * 0.7;
            format!(
                "<path d=\"M{sx} 28c-6-7 6-11 0-19\" fill=\"none\" stroke=\"{}\" stroke-width=\"3\" stroke-linecap=\"round\" opacity=\"0\">{}{}</path>",
                t.muted,
                anim_n("opacity", &[(p, 0.0), (p + 0.8, 0.85), (p + 1.6, 0.0)], 3.0, PLAIN),
                anim("transform", strs(&[(p, "0 6"), (p + 1.6, "0 -6")]), 3.0, TRANSLATE)
            )
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "<g transform=\"translate({} {}) scale({})\">{steam}<ellipse cx=\"38\" cy=\"92\" rx=\"38\" ry=\"6\" fill=\"{}\"/>\
         <path d=\"M62 42h6a10 10 0 0 1 0 20h-6\" fill=\"none\" stroke=\"{tx}\" stroke-width=\"3\"/>\
         <path d=\"M6 34h56v30a20 20 0 0 1-20 20H26A20 20 0 0 1 6 64z\" fill=\"{BMC}\" stroke=\"{tx}\" stroke-width=\"3\" stroke-linejoin=\"round\"/></g>",
        n(x),
        n(y),
        n(s),
        t.border,
        tx = t.text
    )
}

/// The yellow button, with a light sweep across it every few seconds.
fn coffee_button(l: &Lang, (x, y, w, h): (f64, f64, f64, f64), fs: f64) -> String {
    format!(
        "<clipPath id=\"btn\"><rect x=\"{x0}\" y=\"{y0}\" width=\"{w0}\" height=\"{h0}\" rx=\"{rx}\"/></clipPath>\
         <rect x=\"{x0}\" y=\"{y0}\" width=\"{w0}\" height=\"{h0}\" rx=\"{rx}\" fill=\"{BMC}\" stroke=\"#e5c700\"/>\
         <g clip-path=\"url(#btn)\"><rect x=\"{}\" y=\"{y0}\" width=\"34\" height=\"{h0}\" fill=\"#ffffff\" opacity=\"0.5\" transform=\"skewX(-18) translate({} 0)\">{}</rect></g>\
         <text x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"700\" fill=\"#0d1117\" text-anchor=\"middle\">{}</text>",
        n(x - 60.0),
        n(r1(0.325 * y)),
        anim_n("x", &[(0.0, x - 60.0), (0.9, x + w + 40.0)], 3.2, EASE),
        n(x + w / 2.0),
        n(r1(y + h / 2.0 + fs * 0.36)),
        fit(l.button, fs, w - 24.0),
        esc(l.button),
        x0 = n(x),
        y0 = n(y),
        w0 = n(w),
        h0 = n(h),
        rx = n(h / 4.0),
    )
}

fn strip(t: &Theme, l: &Lang) -> String {
    let o = [
        cup(t, 26.0, 8.0, 0.6),
        format!(
            "<text x=\"96\" y=\"36\" font-size=\"{}\" font-weight=\"700\" fill=\"{}\">{}</text>",
            fit(l.strip_title, 16.0, 580.0),
            t.text,
            esc(l.strip_title)
        ),
        format!(
            "<text x=\"96\" y=\"58\" font-size=\"{}\" fill=\"{}\">{}</text>",
            fit(l.strip_sub, 13.5, 580.0),
            t.muted,
            esc(l.strip_sub)
        ),
        coffee_button(l, (690.0, 20.0, 166.0, 40.0), 15.0),
    ];
    svg(
        t,
        880.0,
        80.0,
        "Chisle is free. Its benchmarks aren't. Saved you tokens? Buy me a coffee.",
        &o,
        "",
    )
}

fn support(t: &Theme, l: &Lang) -> String {
    let defs = format!(
        "<radialGradient id=\"warm\" cx=\"0.08\" cy=\"0.5\" r=\"0.5\"><stop offset=\"0\" stop-color=\"{BMC}\" stop-opacity=\"{}\"/><stop offset=\"1\" stop-color=\"{BMC}\" stop-opacity=\"0\"/></radialGradient>",
        n(t.glow)
    );
    let line = |y: u32, text: &str, fs: f64, weight: &str, fill: &str| {
        format!(
            "<text x=\"190\" y=\"{y}\" font-size=\"{}\"{weight} fill=\"{fill}\">{}</text>",
            fit(text, fs, 440.0),
            esc(text)
        )
    };
    let o = [
        "<rect x=\"1\" y=\"1\" width=\"878\" height=\"208\" rx=\"14\" fill=\"url(#warm)\"/>"
            .to_string(),
        cup(t, 50.0, 46.0, 1.15),
        line(84, l.free, 26.0, " font-weight=\"800\"", t.text),
        line(118, l.not_free, 26.0, " font-weight=\"800\"", t.accent),
        line(152, l.body1, 14.5, "", t.muted),
        line(174, l.body2, 14.5, "", t.muted),
        coffee_button(l, (640.0, 76.0, 210.0, 54.0), 17.0),
        format!(
            "<text x=\"745\" y=\"158\"{} fill=\"{}\" text-anchor=\"middle\">buymeacoffee.com/jaypokale3b</text>",
            mono(12.0),
            t.faint
        ),
    ];
    svg(
        t,
        880.0,
        210.0,
        "Chisle is free. Its benchmarks aren't. Every benchmark run costs real API money. Saved you tokens? Buy me a coffee at buymeacoffee.com/jaypokale3b.",
        &o,
        &defs,
    )
}

// ── Line: the travelling divider (one file; it reads on both themes) ──────────
const LINE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="4" viewBox="0 0 1200 4" preserveAspectRatio="none" role="img" aria-label="divider">
<defs><linearGradient id="g" x1="0" x2="1"><stop offset="0" stop-color="#d78a3c" stop-opacity="0"/><stop offset="0.7" stop-color="#f0a85a"/><stop offset="1" stop-color="#fff3e0"/></linearGradient></defs>
<rect width="1200" height="4" rx="2" fill="#d78a3c" opacity="0.18"/>
<rect width="260" height="4" rx="2" fill="url(#g)"><animate attributeName="x" from="-260" to="1200" dur="3.2s" repeatCount="indefinite"/></rect>
</svg>
"##;

type Panel = fn(&Theme, &Lang) -> String;

const PANELS: [(&str, Panel); 9] = [
    ("banner", banner),
    ("stats", stats),
    ("race", race),
    ("trim", trim),
    ("filters", filters),
    ("ladder", ladder),
    ("commands", commands),
    ("strip", strip),
    ("support", support),
];

/// Every file under `assets/readme/`, as (relative path, contents).
#[must_use]
pub fn files() -> Vec<(String, String)> {
    let mut out = vec![("line.svg".to_string(), LINE.to_string())];
    for l in &LANGS {
        let dir = if l.code == "en" {
            String::new()
        } else {
            format!("{}/", l.code)
        };
        let install: &[(&str, Panel)] = if l.code == "en" {
            &[("install", install)]
        } else {
            &[]
        };
        for (name, build) in PANELS.iter().chain(install) {
            for (theme, t) in &THEMES {
                out.push((format!("{dir}{name}-{theme}.svg"), build(t, l)));
            }
        }
    }
    out
}

/// Writes `assets/readme/`, or with `check` reports every stale file.
#[must_use]
pub fn run(root: &Path, check: bool) -> Out {
    let dir = root.join("assets/readme");
    let mut out = Out::default();
    for (rel, svg) in files() {
        let stale = format!("assets/readme/{rel} is stale. Run: npm run build:readme-art");
        if let Some(fail) = crate::sync_file(&dir.join(&rel), &svg, check, &stale) {
            out.stderr += &fail.stderr;
            out.code = 1;
        }
    }
    if out.code == 0 {
        let langs: Vec<&str> = LANGS.iter().map(|l| l.code).collect();
        out.say(if check {
            "README art in sync.".to_string()
        } else {
            format!(
                "wrote assets/readme: line, install, and banner, stats, race, trim, filters, ladder, commands, strip, support for {} (dark, light)",
                langs.join(", ")
            )
        });
    }
    out
}

// ── Strings ───────────────────────────────────────────────────────────────────
const LANGS: [Lang; 5] = [EN, ES, KO, ZH_CN, JA];

fn en_words(a: usize, b: usize) -> String {
    format!("{a} words → {b} words")
}
fn en_cut(p: f64) -> String {
    format!("−{}% vs no ruleset", n(p))
}
fn en_hit(k: usize, a: &str) -> String {
    format!("✓ rung {k}: {a}")
}
fn es_words(a: usize, b: usize) -> String {
    format!("{a} palabras → {b}")
}
fn es_cut(p: f64) -> String {
    format!("−{}% frente a sin ruleset", n(p))
}
fn es_hit(k: usize, a: &str) -> String {
    format!("✓ peldaño {k}: {a}")
}
fn ko_words(a: usize, b: usize) -> String {
    format!("{a}단어 → {b}단어")
}
fn ko_cut(p: f64) -> String {
    format!("규칙 없음 대비 −{}%", n(p))
}
fn ko_hit(k: usize, a: &str) -> String {
    format!("✓ {k}단계: {a}")
}
fn zh_cn_words(a: usize, b: usize) -> String {
    format!("{a} 个词 → {b} 个词")
}
fn zh_cn_cut(p: f64) -> String {
    format!("比无规则集少 {}%", n(p))
}
fn zh_cn_hit(k: usize, a: &str) -> String {
    format!("✓ 第 {k} 级：{a}")
}
fn ja_words(a: usize, b: usize) -> String {
    format!("{a} 語 → {b} 語")
}
fn ja_cut(p: f64) -> String {
    format!("ルールなしより −{}%", n(p))
}
fn ja_hit(k: usize, a: &str) -> String {
    format!("✓ {k} 段目：{a}")
}

const EN: Lang = Lang {
    code: "en",
    pill: "Claude Code plugin · 13 agents · 0 deps",
    tag1: "Fewer tokens in. Fewer tokens out.",
    tag2: "Same answers.",
    words: en_words,
    same: "✓ same answer",
    stats_title: "Chisle vs the same model with no ruleset",
    tiles: [
        ["billed output tokens", "69% of the bare model"],
        ["answer length", "51% of the bare model"],
        ["oversized tool output", "per trimmed tool output"],
        ["accuracy", "bare model: 156/156"],
    ],
    race_title: "Same prompt. Four answers.",
    race_note: "visible answer tokens · lower is leaner",
    bare: "no ruleset",
    tokens: " tokens",
    cut: en_cut,
    race_foot: "Claude Code + Haiku 4.5 · all four in one run · the verbatim answers are below",
    reads: "what Claude reads",
    kept: "  ← error kept",
    foot1: "Raw output is billed again on every later turn",
    foot2: "Oversized tool output: ~48% smaller on average",
    ladder_title: "Before writing code, stop at the first rung that holds",
    rungs: [
        [
            "Does this need to exist at all?",
            "skip it, say so in one line",
        ],
        ["Already in this codebase?", "reuse it"],
        ["Does the stdlib do it?", "use the stdlib"],
        [
            "Native platform feature?",
            "CSS over JS, DB constraint over code",
        ],
        [
            "Already-installed dependency?",
            "use it, never add a new one",
        ],
        ["Can it be one line?", "one line"],
        ["Only then", "the minimum code that works"],
    ],
    request: "Request:",
    reqs: [
        "deep-copy a config object",
        "add a date picker",
        "format a date for the UI",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "reuse utils/formatDate()",
    ],
    hit: en_hit,
    sheet: "cheat sheet",
    notes: [
        "wire every agent on this machine",
        "preview first",
        "put everything back",
        "pause it",
        "over-engineering, one finding per line",
        "bloated code and prose, biggest cut first",
        "tool schemas you pay for but never use",
        "trim MCP output in any agent",
    ],
    filt_title: "Knows the command. Drops the noise.",
    filt_note: "raw output → what Claude reads",
    filt_sub: "Command-aware filters, built in: no extra binary, any OS. Errors, failures and summaries always stay.",
    filt_notes: [
        "256 passing tests hidden, summary kept",
        "400 passes hidden, the failure kept whole",
        "long form rewritten in git's short form",
        "one line per commit, long messages counted",
    ],
    filt_foot: "37 commands: Claude reads 28% of the raw output with Chisle, 84% with RTK. Failed runs filtered in bypass mode; | cat shows raw.",
    chars: "chars",
    strip_title: "Chisle is free. Its benchmarks aren't.",
    strip_sub: "Saved you tokens? A coffee pays for the next benchmark run.",
    free: "Chisle is free.",
    not_free: "Its benchmarks aren't.",
    body1: "Every benchmark run costs real API money.",
    body2: "Saved you tokens? A coffee pays for the next run.",
    button: "☕ Buy me a coffee",
};

const ES: Lang = Lang {
    code: "es",
    pill: "Plugin de Claude Code · 13 agentes · 0 deps",
    tag1: "Menos tokens de entrada y de salida.",
    tag2: "Las mismas respuestas.",
    words: es_words,
    same: "✓ misma respuesta",
    stats_title: "Chisle frente al mismo modelo sin ruleset",
    tiles: [
        ["tokens facturados", "69% del modelo sin reglas"],
        ["longitud de respuesta", "51% del modelo sin reglas"],
        ["salida de herramientas", "por salida recortada"],
        ["precisión", "sin reglas: 156/156"],
    ],
    race_title: "Mismo prompt. Cuatro respuestas.",
    race_note: "tokens de respuesta visibles · menos es mejor",
    bare: "sin ruleset",
    tokens: " tokens",
    cut: es_cut,
    race_foot: "Claude Code + Haiku 4.5 · los cuatro en una ejecución · las respuestas literales, abajo",
    reads: "lo que lee Claude",
    kept: "  ← error conservado",
    foot1: "La salida en bruto se vuelve a cobrar en cada turno",
    foot2: "Salida enorme de herramientas: ~48% más pequeña de media",
    ladder_title: "Antes de escribir código, detente en el primer peldaño que se cumpla",
    rungs: [
        ["¿Hace falta que exista?", "sáltalo y dilo en una línea"],
        ["¿Ya está en este código?", "reutilízalo"],
        [
            "¿Lo hace la biblioteca estándar?",
            "usa la biblioteca estándar",
        ],
        [
            "¿Una función nativa de la plataforma?",
            "CSS antes que JS, restricción de BD antes que código",
        ],
        ["¿Una dependencia ya instalada?", "úsala, nunca añadas otra"],
        ["¿Cabe en una línea?", "una línea"],
        ["Solo entonces", "el código mínimo que funciona"],
    ],
    request: "Petición:",
    reqs: [
        "copia profunda de un objeto de configuración",
        "añadir un selector de fecha",
        "formatear una fecha para la interfaz",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "reutiliza utils/formatDate()",
    ],
    hit: es_hit,
    sheet: "chuleta",
    notes: [
        "configura cada agente de esta máquina",
        "primero, una vista previa",
        "lo deja todo como estaba",
        "lo pausa",
        "sobreingeniería, un hallazgo por línea",
        "código y prosa inflados, el mayor recorte primero",
        "esquemas de herramientas que pagas y no usas",
        "recorta la salida MCP en cualquier agente",
    ],
    filt_title: "Conoce el comando. Quita el ruido.",
    filt_note: "salida en bruto → lo que lee Claude",
    filt_sub: "Filtros por comando, integrados: sin binario extra, en cualquier SO. Errores, fallos y resúmenes siempre se quedan.",
    filt_notes: [
        "256 tests aprobados ocultos, resumen intacto",
        "400 aprobados ocultos, el fallo entero",
        "forma larga reescrita en la forma corta de git",
        "una línea por commit, mensajes largos contados",
    ],
    filt_foot: "37 comandos: Claude lee el 28% de la salida en bruto con Chisle, el 84% con RTK. Los fallos se filtran en modo bypass; | cat muestra el original.",
    chars: "caracteres",
    strip_title: "Chisle es gratis. Sus benchmarks, no.",
    strip_sub: "¿Te ahorró tokens? Un café paga la próxima ejecución del benchmark.",
    free: "Chisle es gratis.",
    not_free: "Sus benchmarks, no.",
    body1: "Cada ejecución de benchmark cuesta dinero real de API.",
    body2: "¿Te ahorró tokens? Un café paga la próxima.",
    button: "☕ Invítame a un café",
};

const KO: Lang = Lang {
    code: "ko",
    pill: "Claude Code 플러그인 · 에이전트 13개 · 의존성 0",
    tag1: "입력 토큰도, 출력 토큰도 적게.",
    tag2: "답은 그대로.",
    words: ko_words,
    same: "✓ 같은 답",
    stats_title: "규칙 세트가 없는 같은 모델과 비교한 Chisle",
    tiles: [
        ["과금 출력 토큰", "규칙 없는 모델의 69%"],
        ["답변 길이", "규칙 없는 모델의 51%"],
        ["큰 도구 출력", "축소된 출력 기준"],
        ["정확도", "규칙 없음: 156/156"],
    ],
    race_title: "같은 프롬프트, 네 가지 답.",
    race_note: "보이는 답변 토큰 · 낮을수록 간결",
    bare: "규칙 없음",
    tokens: " 토큰",
    cut: ko_cut,
    race_foot: "Claude Code + Haiku 4.5 · 넷 다 한 번의 실행 · 원문 답변은 아래에",
    reads: "Claude가 읽는 내용",
    kept: "  ← 오류는 유지",
    foot1: "원본 출력은 이후 매 턴마다 다시 과금됩니다",
    foot2: "큰 도구 출력: 평균 ~48% 감소",
    ladder_title: "코드를 쓰기 전에, 성립하는 첫 단계에서 멈춥니다",
    rungs: [
        ["꼭 있어야 하나?", "건너뛰고 한 줄로 알리기"],
        ["이미 코드베이스에 있나?", "재사용하기"],
        ["표준 라이브러리로 되나?", "표준 라이브러리 쓰기"],
        [
            "플랫폼 기본 기능이 있나?",
            "JS보다 CSS, 앱 코드보다 DB 제약",
        ],
        [
            "이미 설치된 의존성이 있나?",
            "그걸 쓰고, 새로 추가하지 않기",
        ],
        ["한 줄로 되나?", "한 줄로"],
        ["그때서야", "동작하는 최소한의 코드"],
    ],
    request: "요청:",
    reqs: [
        "설정 객체 깊은 복사",
        "날짜 선택기 추가",
        "UI용 날짜 서식 지정",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "utils/formatDate() 재사용",
    ],
    hit: ko_hit,
    sheet: "치트 시트",
    notes: [
        "이 컴퓨터의 모든 에이전트 설정",
        "먼저 미리 보기",
        "전부 되돌리기",
        "일시 중지",
        "과잉 설계, 한 줄에 하나씩",
        "부풀려진 코드와 글, 큰 절감부터",
        "비용만 내고 안 쓰는 도구 스키마",
        "모든 에이전트에서 MCP 출력 축소",
    ],
    filt_title: "명령을 알고, 잡음을 걷어낸다.",
    filt_note: "원본 출력 → Claude가 읽는 것",
    filt_sub: "명령 인식 필터 내장: 추가 바이너리 없이 모든 OS에서. 오류, 실패, 요약은 항상 남긴다.",
    filt_notes: [
        "통과한 테스트 256개 숨김, 요약은 유지",
        "통과 400개 숨김, 실패는 전부 유지",
        "긴 형식을 git의 짧은 형식으로",
        "커밋당 한 줄, 긴 메시지는 줄 수만",
    ],
    filt_foot: "명령 37개: Claude가 읽는 원본 출력은 Chisle 28%, RTK 84%. 실패한 실행도 bypass 모드에서 필터링; | cat 이면 원본.",
    chars: "자",
    strip_title: "Chisle은 무료입니다. 벤치마크는 아닙니다.",
    strip_sub: "토큰을 아꼈나요? 커피 한 잔이 다음 벤치마크 비용이 됩니다.",
    free: "Chisle은 무료입니다.",
    not_free: "벤치마크는 아닙니다.",
    body1: "벤치마크를 돌릴 때마다 실제 API 비용이 듭니다.",
    body2: "토큰을 아꼈나요? 커피 한 잔이 다음 실행 비용이 됩니다.",
    button: "☕ 커피 한 잔 사 주기",
};

const ZH_CN: Lang = Lang {
    code: "zh-CN",
    pill: "Claude Code 插件 · 13 个智能体 · 零依赖",
    tag1: "输入更少 token，输出更少 token。",
    tag2: "答案不变。",
    words: zh_cn_words,
    same: "✓ 答案相同",
    stats_title: "Chisle 对比无规则集的同一模型",
    tiles: [
        ["计费输出 token", "无规则集模型的 69%"],
        ["回答长度", "无规则集模型的 51%"],
        ["超大工具输出", "按每个被精简的输出"],
        ["准确率", "无规则集：156/156"],
    ],
    race_title: "同一个提示词，四个回答。",
    race_note: "可见回答 token · 越低越精简",
    bare: "无规则集",
    tokens: " token",
    cut: zh_cn_cut,
    race_foot: "Claude Code + Haiku 4.5 · 四者同一次运行 · 原始回答见下方",
    reads: "Claude 读到的内容",
    kept: "  ← 保留错误",
    foot1: "原始输出会在之后每一轮被重新计费",
    foot2: "超大工具输出：平均缩小约 48%",
    ladder_title: "写代码之前，停在第一个成立的台阶",
    rungs: [
        ["这真的需要存在吗？", "跳过，并用一句话说明"],
        ["代码库里已经有了吗？", "直接复用"],
        ["标准库能做吗？", "用标准库"],
        [
            "平台原生功能能做吗？",
            "能用 CSS 就不用 JS，能用数据库约束就不写代码",
        ],
        ["已安装的依赖能做吗？", "用它，绝不新增依赖"],
        ["能写成一行吗？", "写成一行"],
        ["最后才", "写能工作的最少代码"],
    ],
    request: "请求：",
    reqs: ["深拷贝一个配置对象", "添加日期选择器", "为界面格式化日期"],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "复用 utils/formatDate()",
    ],
    hit: zh_cn_hit,
    sheet: "速查表",
    notes: [
        "配置本机所有智能体",
        "先预览",
        "全部还原",
        "暂停",
        "过度设计，每行一条",
        "臃肿的代码和文字，收益大的优先",
        "付了费却从不使用的工具 schema",
        "在任何智能体中精简 MCP 输出",
    ],
    filt_title: "认得命令，滤掉噪音。",
    filt_note: "原始输出 → Claude 读到的",
    filt_sub: "内置按命令过滤：无需额外二进制，任何系统可用。错误、失败和汇总始终保留。",
    filt_notes: [
        "隐藏 256 个通过的测试，保留汇总",
        "隐藏 400 个通过，失败完整保留",
        "长格式改写为 git 短格式",
        "每个提交一行，长消息只计行数",
    ],
    filt_foot: "37 条命令：用 Chisle 时 Claude 读原始输出的 28%，用 RTK 时 84%。失败的运行在 bypass 模式下也会过滤；加 | cat 看原始输出。",
    chars: "字符",
    strip_title: "Chisle 是免费的，基准测试可不是。",
    strip_sub: "帮你省下了 token？一杯咖啡就能支付下一次基准测试。",
    free: "Chisle 是免费的。",
    not_free: "基准测试可不是。",
    body1: "每次运行基准测试都要花真实的 API 费用。",
    body2: "帮你省下了 token？一杯咖啡就能支付下一次运行。",
    button: "☕ 请我喝杯咖啡",
};

const JA: Lang = Lang {
    code: "ja",
    pill: "Claude Code プラグイン · 13 エージェント · 依存ゼロ",
    tag1: "入力も出力も、トークンは少なく。",
    tag2: "答えは同じ。",
    words: ja_words,
    same: "✓ 同じ答え",
    stats_title: "ルールなしの同じモデルと比べた Chisle",
    tiles: [
        ["課金出力トークン", "ルールなしモデルの 69%"],
        ["回答の長さ", "ルールなしモデルの 51%"],
        ["巨大なツール出力", "削減した出力あたり"],
        ["精度", "ルールなし：156/156"],
    ],
    race_title: "同じプロンプト、4 つの回答。",
    race_note: "表示される回答トークン · 低いほど簡潔",
    bare: "ルールなし",
    tokens: " トークン",
    cut: ja_cut,
    race_foot: "Claude Code + Haiku 4.5 · 4 つとも同じ実行 · 回答の原文は下に",
    reads: "Claude が読む内容",
    kept: "  ← エラーは残す",
    foot1: "生の出力は以降のターンごとに再課金されます",
    foot2: "巨大なツール出力：平均で約 48% 縮小",
    ladder_title: "コードを書く前に、最初に当てはまる段で止まる",
    rungs: [
        ["そもそも必要か？", "飛ばして、1 行でそう伝える"],
        ["すでにコードベースにあるか？", "再利用する"],
        ["標準ライブラリでできるか？", "標準ライブラリを使う"],
        [
            "プラットフォームの標準機能は？",
            "JS より CSS、アプリより DB 制約",
        ],
        [
            "インストール済みの依存関係は？",
            "それを使い、新しく足さない",
        ],
        ["1 行で書けるか？", "1 行で書く"],
        ["そこで初めて", "動く最小限のコード"],
    ],
    request: "リクエスト：",
    reqs: [
        "設定オブジェクトをディープコピー",
        "日付ピッカーを追加",
        "UI 用に日付を整形",
    ],
    answers: [
        "structuredClone(config)",
        "<input type=\"date\">",
        "utils/formatDate() を再利用",
    ],
    hit: ja_hit,
    sheet: "チートシート",
    notes: [
        "このマシンの全エージェントを設定",
        "まずプレビュー",
        "すべて元に戻す",
        "一時停止",
        "過剰設計を 1 行ずつ指摘",
        "膨らんだコードと文章、効果の大きい順",
        "払っているのに使わないツールスキーマ",
        "どのエージェントでも MCP 出力を削減",
    ],
    filt_title: "コマンドを見て、ノイズを落とす。",
    filt_note: "生の出力 → Claude が読む内容",
    filt_sub: "コマンド別フィルターを内蔵：追加バイナリ不要、どの OS でも。エラー・失敗・要約は必ず残す。",
    filt_notes: [
        "成功したテスト 256 件を非表示、要約は残す",
        "成功 400 件を非表示、失敗はそのまま",
        "長い形式を git の短い形式に",
        "コミットごとに 1 行、長いメッセージは行数のみ",
    ],
    filt_foot: "37 コマンド：Claude が読む生の出力は Chisle で 28%、RTK で 84%。失敗した実行も bypass モードでフィルター；| cat で生の出力。",
    chars: "文字",
    strip_title: "Chisle は無料。ベンチマークは無料じゃない。",
    strip_sub: "トークンを節約できた？コーヒー 1 杯で次のベンチマークが回せます。",
    free: "Chisle は無料。",
    not_free: "ベンチマークは無料じゃない。",
    body1: "ベンチマークを回すたびに実際の API 料金がかかります。",
    body2: "トークンを節約できた？コーヒー 1 杯で次の実行が回せます。",
    button: "☕ コーヒーをおごる",
};
