// SPDX-License-Identifier: MIT

//! The support strip and card.

use super::{EASE, Lang, PLAIN, TRANSLATE, Theme, anim, anim_n, esc, fit, mono, n, r1, strs, svg};

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
fn coffee_button(l: &Lang, rect: [f64; 4], fs: f64) -> String {
    let [x, y, w, h] = rect;
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

pub(super) fn strip(t: &Theme, l: &Lang) -> String {
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
        coffee_button(l, [690.0, 20.0, 166.0, 40.0], 15.0),
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

pub(super) fn support(t: &Theme, l: &Lang) -> String {
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
        coffee_button(l, [640.0, 76.0, 210.0, 54.0], 17.0),
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
