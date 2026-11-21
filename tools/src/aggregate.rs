// SPDX-License-Identifier: MIT

//! Aggregates `benchmarks/results/raw/*.json` (written by `run-live.sh`) into
//! a measured comparison table across the four arms.
//!
//! Metrics per cell: `out_tokens`, the model's `usage.output_tokens` (the real
//! generation cost); `answer_tok`, the visible answer size as chars/4 (what
//! the user reads); and `lines`, the answer line count (a coding-bloat proxy).

use crate::js::{self, num};
use crate::{Ctx, Out};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Keys match the committed raw filenames (`<task>__<arm>.json`).
const ARMS: [&str; 4] = ["vanilla", "caveman", "ponytail", "chisle"];
const SEGMENTS: [&str; 3] = ["coding", "noncoding", "all"];

/// Task kinds: "coding" wants working code back; "noncoding" wants an
/// explanation. Classified from what the prompt asks for, not from whether
/// the answer happens to contain a fenced block. The single source of truth:
/// `samples` reads it too.
///
/// palindrome is the one genuine borderline: it asks for a one-liner, so it
/// is filed as coding here, while the Sonnet writeup groups it with the small
/// prose prompts. Flipping it moves the coding/non-coding totals by ~1pt and
/// changes no conclusion; see the sensitivity note in the README.
pub const KIND: [(&str, &str); 22] = [
    ("debounce", "coding"),
    ("cache", "coding"),
    ("auth-bug", "coding"),
    ("pooling", "noncoding"),
    ("rest-graphql", "noncoding"),
    ("regex-concept", "noncoding"),
    // Sonnet suite
    ("thread", "coding"),
    ("retry", "coding"),
    ("mixed", "coding"),
    ("signup", "coding"),
    ("ratelimit", "coding"),
    ("caching", "coding"),
    ("palindrome", "coding"),
    ("deadlock", "noncoding"),
    ("letconst", "noncoding"),
    // SUITE=large (run-live.sh), first run 2026-10-01
    ("migration", "coding"),
    ("statemachine", "coding"),
    ("csvpipeline", "coding"),
    ("authflow", "coding"),
    ("apidesign", "noncoding"),
    ("postmortem", "noncoding"),
    ("architecture", "noncoding"),
];

#[must_use]
pub fn kind(task: &str) -> Option<&'static str> {
    KIND.iter().find(|(t, _)| *t == task).map(|(_, k)| *k)
}

/// Visible answer tokens, estimated as characters / 4 after collapsing whitespace.
#[must_use]
pub fn est_tok(s: &str) -> f64 {
    let mut collapsed = String::with_capacity(s.len());
    for c in s.chars() {
        if js::is_space(c) {
            if !collapsed.ends_with(' ') {
                collapsed.push(' ');
            }
        } else {
            collapsed.push(c);
        }
    }
    js::round(js::len(js::trim(&collapsed)) as f64 / 4.0)
}

/// Lines of a trimmed answer; an empty answer has none.
#[must_use]
pub fn lines(text: &str) -> f64 {
    let t = js::trim(text);
    if t.is_empty() {
        0.0
    } else {
        t.split('\n').count() as f64
    }
}

impl Cell {
    /// The tokens as `+=` adds them: missing is NaN, null is 0.
    fn out_num(&self) -> f64 {
        match &self.out {
            Some(Value::Null) => 0.0,
            v => js::number(v.as_ref()),
        }
    }
}

struct Cell {
    task: String,
    arm: String,
    kind: &'static str,
    /// `usage.output_tokens` as read: `None` where the field is missing.
    out: Option<Value>,
    ans: f64,
    lines: f64,
}

fn load(ctx: &Ctx) -> Vec<Cell> {
    let raw = ctx.raw_dir("benchmarks/results/raw");
    let mut cells = Vec::new();
    for f in js::read_dir(&raw).unwrap_or_default() {
        let Some((task, arm)) = js::split_cell(&f, js::is_word) else {
            continue;
        };
        let Some(d) = js::read_json(&raw.join(&f)) else {
            continue;
        };
        if !js::truthy(Some(&d)) || js::truthy(d.get("is_error")) {
            continue;
        }
        let result = match d.get("result") {
            Some(Value::String(s)) => s.as_str(),
            _ => "",
        };
        let out = if js::truthy(d.get("usage")) {
            d["usage"].get("output_tokens").cloned()
        } else {
            Some(0.into())
        };
        cells.push(Cell {
            task: task.into(),
            arm: arm.into(),
            kind: kind(task).unwrap_or("other"),
            out,
            ans: est_tok(result),
            lines: lines(result),
        });
    }
    cells
}

fn pct(base: f64, v: f64) -> String {
    // `base ? … : 0`: NaN is falsy too.
    num(if base == 0.0 || base.is_nan() {
        0.0
    } else {
        js::round((v / base) * 100.0)
    })
}

#[derive(Default, Clone, Copy)]
struct Sum {
    out: f64,
    ans: f64,
    lines: f64,
    n: f64,
}

impl Sum {
    fn get(self, metric: &str) -> f64 {
        match metric {
            "out" => self.out,
            "ans" => self.ans,
            _ => self.lines,
        }
    }
}

/// Prints the tables, or the cells and sums as JSON with `json`.
#[must_use]
pub fn run(ctx: &Ctx, json: bool) -> Out {
    let cells = load(ctx);
    if cells.is_empty() {
        return Out::fail("no raw results yet — run: bash benchmarks/run-live.sh");
    }
    let mut sum: HashMap<&str, HashMap<&str, Sum>> = ARMS
        .iter()
        .map(|a| (*a, SEGMENTS.iter().map(|s| (*s, Sum::default())).collect()))
        .collect();
    for c in &cells {
        let Some(arm) = sum.get_mut(c.arm.as_str()) else {
            continue;
        };
        for seg in [c.kind, "all"] {
            // An "other" task counts towards "all" only; the JavaScript
            // version threw a TypeError on it instead.
            if let Some(s) = arm.get_mut(seg) {
                s.out += c.out_num();
                s.ans += c.ans;
                s.lines += c.lines;
                s.n += 1.0;
            }
        }
    }
    let mut out = Out::default();
    if json {
        out.say(js::stringify(&to_json(&cells, &sum), 2));
        return out;
    }
    let mut by_task: Vec<(&str, HashMap<&str, &Cell>)> = Vec::new();
    for c in &cells {
        let i = by_task
            .iter()
            .position(|(t, _)| *t == c.task)
            .unwrap_or_else(|| {
                by_task.push((&c.task, HashMap::new()));
                by_task.len() - 1
            });
        by_task[i].1.insert(&c.arm, c);
    }
    by_task.sort_by(|(a, _), (b, _)| {
        js::locale_compare(kind(a).unwrap_or(""), kind(b).unwrap_or(""))
            .then_with(|| js::locale_compare(a, b))
    });
    tables(&mut out, &by_task, &sum);
    out
}

fn to_json(cells: &[Cell], sum: &HashMap<&str, HashMap<&str, Sum>>) -> Value {
    let cells: Vec<Value> = cells
        .iter()
        .map(|c| {
            let mut v = Map::new();
            v.insert("task".into(), json!(c.task));
            v.insert("arm".into(), json!(c.arm));
            v.insert("kind".into(), json!(c.kind));
            // `JSON.stringify` leaves out an undefined field.
            if let Some(out) = &c.out {
                v.insert("out".into(), out.clone());
            }
            v.insert("ans".into(), js::json_num(c.ans));
            v.insert("lines".into(), js::json_num(c.lines));
            Value::Object(v)
        })
        .collect();
    let mut sums = Map::new();
    for arm in ARMS {
        let mut segs = Map::new();
        for seg in SEGMENTS {
            let s = sum[arm][seg];
            segs.insert(
                seg.into(),
                json!({"out": js::json_num(s.out), "ans": js::json_num(s.ans),
                    "lines": js::json_num(s.lines), "n": js::json_num(s.n)}),
            );
        }
        sums.insert(arm.into(), Value::Object(segs));
    }
    json!({"cells": cells, "sum": sums})
}

const HEAD: &str = "| task | kind | vanilla | caveman | ponytail | chisle |\n|------|------|--------:|--------:|---------:|---------:|";
const SEG_HEAD: &str = "| segment | vanilla | caveman | ponytail | chisle |\n|---------|--------:|--------:|---------:|---------:|";

fn tables(
    out: &mut Out,
    by_task: &[(&str, HashMap<&str, &Cell>)],
    sum: &HashMap<&str, HashMap<&str, Sum>>,
) {
    let per_task = |out: &mut Out, title: &str, pick: fn(&Cell) -> String| {
        out.say(title);
        out.say(HEAD);
        for (t, arms) in by_task {
            let row: Vec<String> = ARMS
                .iter()
                .map(|a| arms.get(a).map_or("—".into(), |c| pick(c)))
                .collect();
            out.say(format!(
                "| {t} | {} | {} |",
                kind(t).unwrap_or("undefined"),
                row.join(" | ")
            ));
        }
    };
    per_task(
        out,
        "## Per-task billed output tokens (usage.output_tokens — includes model reasoning)\n",
        |c| js::string(c.out.as_ref()),
    );
    per_task(
        out,
        "\n## Per-task visible answer size (lines of the delivered answer)\n",
        |c| num(c.lines),
    );
    segment_tables(out, sum);
}

/// Each metric as % of vanilla by segment, then the raw totals.
fn segment_tables(out: &mut Out, sum: &HashMap<&str, HashMap<&str, Sum>>) {
    for (metric, label) in [
        ("out", "billed output tokens"),
        ("ans", "visible answer tokens"),
        ("lines", "answer lines"),
    ] {
        out.say(format!(
            "\n## {label} as % of vanilla baseline (lower = leaner)\n"
        ));
        out.say(SEG_HEAD);
        for seg in SEGMENTS {
            let base = sum["vanilla"][seg].get(metric);
            let row: Vec<String> = ARMS
                .iter()
                .map(|a| format!("{}%", pct(base, sum[a][seg].get(metric))))
                .collect();
            out.say(format!("| {seg} | {} |", row.join(" | ")));
        }
    }
    out.say("\n## Raw totals (billed tokens / answer lines, n cells)\n");
    out.say(SEG_HEAD);
    for seg in SEGMENTS {
        let row: Vec<String> = ARMS
            .iter()
            .map(|a| {
                let s = sum[a][seg];
                format!("{}t/{}L (n={})", num(s.out), num(s.lines), num(s.n))
            })
            .collect();
        out.say(format!("| {seg} | {} |", row.join(" | ")));
    }
}
