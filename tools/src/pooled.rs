// SPDX-License-Identifier: MIT

//! The pooled scorer behind the README tables: the ratio of summed tokens to
//! vanilla across every cell in the given seed directories, a 95% bootstrap
//! over cells, and the same figures on rows where every arm delivered an
//! answer (non-answers such as "which stack?" or "I need your code" are cheap
//! and flatter any arm).

use crate::js::{self, num};
use crate::{Ctx, Out};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const CODING: [&str; 6] = [
    "debounce",
    "cache",
    "migration",
    "statemachine",
    "csvpipeline",
    "authflow",
];
/// Bootstrap resamples.
const B: usize = 5000;

fn delivered(task: &str, r: &str) -> bool {
    if task == "auth-bug" {
        r.contains(">=") || r.contains("<=")
    } else if CODING.contains(&task) {
        r.contains("```")
            || r.contains("<parameter name=\"content\">")
            || r.contains("<parameter name=\"contents\">")
    } else {
        let t = js::trim(r);
        js::len(r) > 150
            && !["I need", "Need", "Can you", "What "]
                .iter()
                .any(|p| t.starts_with(p))
    }
}

#[derive(Clone, Copy)]
struct Cell {
    billed: f64,
    think: f64,
    vis: f64,
    lines: f64,
    ok: bool,
}

impl Cell {
    fn get(self, k: Metric) -> f64 {
        match k {
            Metric::Billed => self.billed,
            Metric::Think => self.think,
            Metric::Vis => self.vis,
            Metric::Lines => self.lines,
        }
    }
}

#[derive(Clone, Copy)]
enum Metric {
    Billed,
    Think,
    Vis,
    Lines,
}

type Row = HashMap<String, Cell>;

/// The first model's `thinkingTokens`, or 0.
fn thinking(j: &Value) -> f64 {
    let first = j
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|m| m.values().next());
    js::number_or_zero(first.and_then(|m| m.get("thinkingTokens")))
}

fn load(dirs: &[String], arms: &[&str]) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for d in dirs {
        let names = js::read_dir(Path::new(d)).map_err(|e| format!("{d}: {e}"))?;
        let mut tasks: Vec<&str> = Vec::new();
        for f in names.iter().filter(|f| f.ends_with(".json")) {
            let t = f.split("__").next().unwrap_or_default();
            if !tasks.contains(&t) {
                tasks.push(t);
            }
        }
        for t in tasks {
            let mut row = Row::new();
            let mut missing = false;
            for a in arms {
                let Some(j) = js::read_json(&Path::new(d).join(format!("{t}__{a}.json"))) else {
                    missing = true;
                    continue;
                };
                let r = j.get("result").and_then(Value::as_str).unwrap_or_default();
                let billed = js::number(j.get("usage").and_then(|u| u.get("output_tokens")));
                let think = thinking(&j);
                let lines = js::trim(r).split('\n').count() as f64;
                row.insert(
                    (*a).into(),
                    Cell {
                        billed,
                        think,
                        vis: billed - think,
                        lines,
                        ok: delivered(t, r),
                    },
                );
            }
            if !missing {
                rows.push(row);
            }
        }
    }
    Ok(rows)
}

fn ratio(set: &[&Row], a: &str, k: Metric) -> f64 {
    set.iter().fold(0.0, |s, r| s + r[a].get(k))
        / set.iter().fold(0.0, |s, r| s + r["vanilla"].get(k))
}

/// A fixed-seed `SplitMix64`, so a rerun prints the same interval.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        // The top 53 bits as a fraction of 1, then scaled: Math.random() * n | 0.
        let unit = (z >> 11) as f64 / (1u64 << 53) as f64;
        (unit * n as f64) as usize
    }
}

fn ci(rng: &mut Rng, set: &[&Row], a: &str, k: Metric) -> String {
    let mut xs: Vec<f64> = (0..B)
        .map(|_| {
            let (mut s, mut v) = (0.0, 0.0);
            for _ in 0..set.len() {
                let r = set[rng.below(set.len())];
                s += r[a].get(k);
                v += r["vanilla"].get(k);
            }
            s / v
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    let at = |q: f64| num(js::round(100.0 * xs[(B as f64 * q) as usize]));
    format!("{}–{}", at(0.025), at(0.975))
}

fn pct(x: f64) -> String {
    format!("{}%", num(js::round(100.0 * x)))
}

/// Prints the pooled tables for the seed directories in `dirs`.
#[must_use]
pub fn run(ctx: &Ctx, dirs: &[String]) -> Out {
    let arms: Vec<&str> = ctx
        .var("ARMS")
        .unwrap_or("vanilla,caveman,ponytail,chisle")
        .split(',')
        .collect();
    let rows = match load(dirs, &arms) {
        Ok(rows) => rows,
        Err(e) => return Out::fail(e),
    };
    let mut rng = Rng(0x00c0_ffee);
    let mut out = Out::default();
    let all: Vec<&Row> = rows.iter().collect();
    let answered: Vec<&Row> = rows
        .iter()
        .filter(|r| arms.iter().all(|a| r[*a].ok))
        .collect();
    for (label, set) in [("all cells", &all), ("every arm delivered", &answered)] {
        out.say(format!("\n{label} (n={})", set.len()));
        let head: Vec<String> = HEAD.iter().map(|(h, w)| js::pad(h, *w, true)).collect();
        out.say(format!("{} {}", js::pad("arm", 9, false), head.join(" ")));
        for a in &arms[1..] {
            out.say(arm_line(&mut rng, set, a));
        }
    }
    let misses: Vec<String> = arms
        .iter()
        .map(|a| format!("{a} {}", rows.iter().filter(|r| !r[*a].ok).count()))
        .collect();
    out.say(format!("\nnon-answers: {}", misses.join(", ")));
    out
}

/// The table's columns and their widths.
const HEAD: [(&str, usize); 7] = [
    ("answer len", 11),
    ("CI", 9),
    ("lines", 6),
    ("billed", 7),
    ("CI", 9),
    ("think", 6),
    ("backfires", 10),
];

/// One arm's row: its ratios to vanilla, two bootstrap intervals and its backfires.
fn arm_line(rng: &mut Rng, set: &[&Row], a: &str) -> String {
    let backfires = set
        .iter()
        .filter(|r| r[a].billed > r["vanilla"].billed)
        .count();
    let cols = [
        pct(ratio(set, a, Metric::Vis)),
        ci(rng, set, a, Metric::Vis),
        pct(ratio(set, a, Metric::Lines)),
        pct(ratio(set, a, Metric::Billed)),
        ci(rng, set, a, Metric::Billed),
        pct(ratio(set, a, Metric::Think)),
        format!("{backfires}/{}", set.len()),
    ];
    let cols: Vec<String> = cols
        .iter()
        .zip(HEAD)
        .map(|(c, (_, w))| js::pad(c, w, true))
        .collect();
    format!("{} {}", js::pad(a, 9, false), cols.join(" "))
}
