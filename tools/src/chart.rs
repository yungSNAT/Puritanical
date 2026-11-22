// SPDX-License-Identifier: Apache-2.0 AND MIT

//! `assets/benchmark.svg` (dark) and `assets/benchmark-light.svg` from real
//! measured results: billed output tokens per ruleset as % of the same model
//! with no ruleset, from the side-by-side run (Chisle 3.14 and 3.13, which
//! share one ruleset, caveman and ponytail in one session, Haiku 4.5,
//! 2026-11-19). Pooled sums over every task where all arms have a cell, the
//! same method as `pooled` and the README table.
//!
//! Source: `benchmarks/results/raw-314-sidebyside/<suite>-s<seed>/<task>__<arm>.json`.
//!
//! A second chart, `assets/benchmark-terminal(-light).svg`, shows the input
//! axis: terminal output Claude reads, as % of raw, over the 37-command corpus
//! in `benchmarks/results/raw-314-commands/commands.json`
//! (`benchmarks/replay-commands.js --json`). caveman and ponytail are rulesets
//! with no tool-output hook, so they read 100% by construction; RTK is the
//! measured rival there.

use crate::js::{self, num};
use crate::{Out, sync_file};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

const SANS: &str =
    "-apple-system,BlinkMacSystemFont,'Segoe UI','Noto Sans',Helvetica,Arial,sans-serif";

struct Theme {
    suffix: &'static str,
    bg: &'static str,
    border: &'static str,
    title: &'static str,
    text: &'static str,
    muted: &'static str,
    accent: &'static str,
    /// Bar tones follow `ARMS` order: two rivals, the previous Chisle, the current one.
    bars: [&'static str; 4],
}

// The README swaps these with <picture>.
const THEMES: [Theme; 2] = [
    Theme {
        suffix: "",
        bg: "#0d1117",
        border: "#30363d",
        title: "#e6edf3",
        text: "#c9d1d9",
        muted: "#8b949e",
        accent: "#f0a85a",
        bars: ["#484f58", "#484f58", "#7d5530", "#d78a3c"],
    },
    Theme {
        suffix: "-light",
        bg: "#ffffff",
        border: "#d0d7de",
        title: "#1f2328",
        text: "#31363c",
        muted: "#59636e",
        accent: "#b35a14",
        bars: ["#afb8c1", "#afb8c1", "#ecc49b", "#d78a3c"],
    },
];

/// Our own arm. The key is the token in the committed raw filenames
/// (`<task>__chisle.json`); renaming it orphans the result cells and zeroes
/// our bar, so reference it through this constant.
const SELF: &str = "chisle";
const ARMS: [(&str, &str); 4] = [
    ("caveman", "caveman"),
    ("ponytail", "ponytail"),
    ("chisle-3.13", "Chisle 3.13"),
    (SELF, "Chisle"),
];
const TERM_ARMS: [(&str, &str); 4] = [
    ("caveman", "caveman"),
    ("ponytail", "ponytail"),
    ("rtk", "RTK"),
    (SELF, "Chisle"),
];

/// One chart's figures, in output key order.
type Stat = Vec<(&'static str, f64)>;

fn get(stat: &Stat, key: &str) -> f64 {
    stat.iter()
        .find(|(k, _)| *k == key)
        .map_or(f64::NAN, |(_, v)| *v)
}

fn collect_terminal(root: &Path) -> Result<Stat, String> {
    let path = root.join("benchmarks/results/raw-314-commands/commands.json");
    let rows = js::read_json(&path).ok_or_else(|| format!("cannot read {}", path.display()))?;
    let rows = rows.as_array().ok_or("commands.json is not an array")?;
    let sum = |k: &str| {
        rows.iter().fold(0.0, |s, r| {
            s + js::number(r.get("chars").and_then(|c| c.get(k)))
        })
    };
    let pct = |k: &str| js::round((100.0 * sum(k)) / sum("bare"));
    let held = rows.iter().filter(|r| js::truthy(r.get("held"))).count();
    Ok(vec![
        ("n", rows.len() as f64),
        ("held", held as f64),
        ("caveman", 100.0),
        ("ponytail", 100.0),
        ("rtk", pct("rtk")),
        (SELF, pct(SELF)),
    ])
}

fn arm_char(c: char) -> bool {
    js::is_word(c) || c == '.' || c == '-'
}

/// One run directory's output tokens, by task, then by arm.
fn run_cells(dir: &Path) -> Map<String, Value> {
    let mut cells: Map<String, Value> = Map::new();
    for f in &js::read_dir(dir).unwrap_or_default() {
        let Some((task, arm)) = js::split_cell(f, arm_char) else {
            continue;
        };
        let Some(j) = js::read_json(&dir.join(f)) else {
            continue;
        };
        if js::truthy(Some(&j)) && js::truthy(j.get("usage")) {
            let tokens = j["usage"]
                .get("output_tokens")
                .cloned()
                .unwrap_or(Value::Null);
            let cell = cells
                .entry(task)
                .or_insert_with(|| Value::Object(Map::new()));
            cell[arm] = tokens;
        }
    }
    cells
}

fn collect(root: &Path) -> Stat {
    let raw = root.join("benchmarks/results/raw-314-sidebyside");
    let mut rows: Vec<BTreeMap<String, f64>> = Vec::new();
    for d in js::read_dir(&raw).unwrap_or_default() {
        for c in run_cells(&raw.join(&d)).values() {
            let keys = std::iter::once("vanilla").chain(ARMS.iter().map(|a| a.0));
            let present = keys.clone().all(|k| c.get(k).is_some_and(|v| !v.is_null()));
            if present {
                rows.push(
                    keys.map(|k| (k.to_string(), js::number(c.get(k))))
                        .collect(),
                );
            }
        }
    }
    let base = rows.iter().fold(0.0, |s, c| s + c["vanilla"]);
    let mut stat: Stat = vec![("n", rows.len() as f64)];
    for (key, _) in ARMS {
        let v = if base == 0.0 {
            0.0
        } else {
            js::round(100.0 * rows.iter().fold(0.0, |s, c| s + c[key]) / base)
        };
        stat.push((key, v));
    }
    stat
}

struct Spec {
    arms: [(&'static str, &'static str); 4],
    suffix: &'static str,
    line: &'static str,
    title: &'static str,
    sub: fn(&Stat) -> String,
    aria: fn(&Stat) -> String,
    terminal: bool,
}

const CHARTS: [Spec; 2] = [
    Spec {
        arms: ARMS,
        suffix: "",
        line: "100% = no ruleset",
        title: "Claude Code — billed output tokens, % of the same model with no ruleset",
        sub: |s| {
            format!(
                "Haiku 4.5, {} answers, all arms in one run; lower = cheaper; 3.13 and 3.14 share one ruleset",
                num(get(s, "n"))
            )
        },
        aria: |s| {
            format!(
                "Claude Code benchmark, {} answers in one run, billed output tokens as percent of the same model with no ruleset.",
                num(get(s, "n"))
            )
        },
        terminal: false,
    },
    Spec {
        arms: TERM_ARMS,
        suffix: "-terminal",
        line: "100% = raw output",
        title: "Terminal output Claude reads, % of the raw output",
        sub: |s| {
            format!(
                "{} shell commands ({} held out), Claude Code limits; caveman and ponytail never touch tool output",
                num(get(s, "n")),
                num(get(s, "held"))
            )
        },
        aria: |s| {
            format!(
                "Terminal output Claude reads across {} shell commands, as percent of the raw output.",
                num(get(s, "n"))
            )
        },
        terminal: true,
    },
];

const LEFT: f64 = 170.0;
const TOP: f64 = 92.0;
const ROW_H: f64 = 50.0;
/// Pixels per percentage point: the plot is 600 wide and runs to 110%.
const PX: f64 = 600.0 / 110.0;

fn build_svg(stat: &Stat, th: &Theme, spec: &Spec) -> String {
    let (left, top, row_h) = (LEFT, TOP, ROW_H);
    let line100 = left + 100.0 * PX;
    let arms = spec.arms;
    // RTK is a rival, not an older Chisle.
    let bars = if spec.terminal {
        [th.bars[0], th.bars[0], th.bars[0], th.bars[3]]
    } else {
        th.bars
    };
    let mut body = format!(
        "<line x1=\"{l}\" y1=\"{}\" x2=\"{l}\" y2=\"{}\" stroke=\"{m}\" stroke-width=\"1.5\" stroke-dasharray=\"4 3\"/>\
         <text x=\"{l}\" y=\"{}\" font-size=\"11\" fill=\"{m}\" text-anchor=\"middle\">{}</text>",
        num(top - 14.0),
        num(top + arms.len() as f64 * row_h - 10.0),
        num(top - 20.0),
        spec.line,
        l = num(line100),
        m = th.muted,
    );
    for (i, &(key, label)) in arms.iter().enumerate() {
        body.push_str(&bar_row(th, i, (key, label), get(stat, key), bars[i]));
    }
    frame(th, spec, stat, &body)
}

/// Arm `i`'s label, its bar growing in and its percentage.
fn bar_row(th: &Theme, i: usize, arm: (&str, &str), v: f64, tone: &str) -> String {
    let (key, label) = arm;
    let y = TOP + i as f64 * ROW_H;
    let mine = key == SELF;
    let bold = if mine { " font-weight=\"700\"" } else { "" };
    let fill = if mine { th.accent } else { th.text };
    let bar = num(f64::max(2.0, v * PX));
    let delay = js::to_fixed(0.12 * i as f64, 2);
    format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"14\" fill=\"{fill}\" text-anchor=\"end\"{bold}>{label}</text>\
         <rect x=\"{}\" y=\"{}\" width=\"{bar}\" height=\"28\" rx=\"5\" fill=\"{tone}\">\
         <animate attributeName=\"width\" values=\"0;{bar}\" dur=\"0.9s\" begin=\"{delay}s\" fill=\"freeze\" calcMode=\"spline\" keySplines=\"0.2 0.7 0.2 1\"/></rect>\
         <text x=\"{}\" y=\"{}\" font-size=\"13\" fill=\"{fill}\"{bold}>{}%</text>",
        num(LEFT - 12.0),
        num(y + 21.0),
        num(LEFT),
        num(y + 2.0),
        num(LEFT + f64::max(2.0, v * PX) + 9.0),
        num(y + 21.0),
        num(v),
    )
}

/// The card, title and subtitle around the plot `body`.
fn frame(th: &Theme, spec: &Spec, stat: &Stat, body: &str) -> String {
    let (w, h) = (860.0, 300.0);
    let summary: Vec<String> = spec
        .arms
        .iter()
        .map(|(k, l)| format!("{l} {}%", num(get(stat, k))))
        .collect();
    let aria = format!("{} {}.", (spec.aria)(stat), summary.join(", "));
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"{SANS}\" role=\"img\" aria-label=\"{aria}\">
<rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"14\" fill=\"{}\" stroke=\"{}\"/>
<text x=\"{}\" y=\"34\" font-size=\"17\" font-weight=\"700\" fill=\"{}\" text-anchor=\"middle\">{}</text>
<text x=\"{}\" y=\"54\" font-size=\"11.5\" fill=\"{}\" text-anchor=\"middle\">{}</text>
{body}</svg>
",
        num(w - 1.0),
        num(h - 1.0),
        th.bg,
        th.border,
        num(w / 2.0),
        th.title,
        spec.title,
        num(w / 2.0),
        th.muted,
        (spec.sub)(stat),
        w = num(w),
        h = num(h),
    )
}

fn stat_json(stat: &Stat) -> Value {
    Value::Object(
        stat.iter()
            .map(|(k, v)| ((*k).to_string(), js::json_num(*v)))
            .collect(),
    )
}

/// Writes the four chart files, checks them with `check`, or prints the figures with `json`.
#[must_use]
pub fn run(root: &Path, check: bool, json: bool) -> Out {
    let stats = match collect_terminal(root) {
        Ok(terminal) => [collect(root), terminal],
        Err(e) => return Out::fail(e),
    };
    let mut out = Out::default();
    if json {
        out.say(js::stringify(
            &Value::Array(stats.iter().map(stat_json).collect()),
            2,
        ));
        return out;
    }
    for (spec, stat) in CHARTS.iter().zip(&stats) {
        for th in &THEMES {
            let name = format!("benchmark{}{}.svg", spec.suffix, th.suffix);
            let svg = build_svg(stat, th, spec);
            let stale = format!("assets/{name} is stale. Run: npm run build:chart");
            if let Some(fail) = sync_file(&root.join("assets").join(&name), &svg, check, &stale) {
                return fail;
            }
            if !check {
                let summary: Vec<String> = spec
                    .arms
                    .iter()
                    .map(|(k, l)| format!("{l} {}%", num(get(stat, k))))
                    .collect();
                out.say(format!("wrote assets/{name}: {}", summary.join(", ")));
            }
        }
    }
    if check {
        out.say("chart in sync.");
    }
    out
}
