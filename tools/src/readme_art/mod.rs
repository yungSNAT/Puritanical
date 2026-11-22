// SPDX-License-Identifier: Apache-2.0 AND MIT

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

mod banner;
mod coffee;
mod i18n;
mod ladder;
mod race;

use banner::{banner, stats};
use coffee::{strip, support};
use i18n::LANGS;
use ladder::{commands, install, ladder};
use race::{filters, race, trim};

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
