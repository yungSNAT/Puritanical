// SPDX-License-Identifier: MIT

//! Scores `benchmarks/agentic/raw/*.json` into a correctness-first table.
//!
//! pass: the hidden test the agent never saw; files: new files created
//! (YAGNI); loc: net lines added (shortest working diff); deps: dependencies
//! added to package.json (ladder rung 5); tokens: billed output and total
//! context read (the cost side).

use crate::js::{self, num, to_fixed};
use crate::stats::{fisher, fmt_p, wilson};
use crate::{Ctx, Out};
use serde_json::Value;
use std::collections::HashSet;

fn text(r: &Value, k: &str) -> String {
    js::string(r.get(k))
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().fold(0.0, |a, b| a + b) / xs.len() as f64
    }
}

/// `(v || []).length`.
fn count(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Array(a)) => a.len() as f64,
        Some(Value::String(s)) => js::len(s) as f64,
        v if !js::truthy(v) => 0.0,
        _ => f64::NAN,
    }
}

fn passed(c: &[&Value]) -> f64 {
    c.iter().filter(|r| js::truthy(r.get("pass"))).count() as f64
}

fn rule(label: &str) -> String {
    "-".repeat(js::len(label) + 1) + ":"
}

fn ci(k: f64, n: f64) -> String {
    wilson(k, n).map(|x| to_fixed(100.0 * x, 1) + "%").join("–")
}

/// Prints the tables, or every cell as JSON with `json`.
#[must_use]
pub fn run(ctx: &Ctx, json: bool) -> Out {
    let raw = ctx.raw_dir("benchmarks/agentic/raw");
    let arms: Vec<String> = ctx
        .var("ARMS")
        .unwrap_or("vanilla,chisle")
        .split(',')
        .map(|a| js::trim(a).to_string())
        .filter(|a| !a.is_empty())
        .collect();
    let rows: Vec<Value> = js::read_dir(&raw)
        .unwrap_or_default()
        .iter()
        .filter(|f| f.ends_with(".json"))
        .filter_map(|f| js::read_json(&raw.join(f)))
        .collect();
    if rows.is_empty() {
        return Out::fail("no results — run: node benchmarks/agentic/run.js");
    }
    let mut out = Out::default();
    if json {
        out.say(js::stringify(&Value::Array(rows), 1));
        return out;
    }
    report(&mut out, &rows, &arms);
    out
}

fn report(out: &mut Out, rows: &[Value], arms: &[String]) {
    let treat: Vec<&String> = arms.iter().filter(|a| *a != "vanilla").collect();
    // Only compare (fixture, seed) pairs that completed in every arm. A run
    // cut short mid-suite otherwise leaves the arms with different seed sets,
    // and the comparison silently stops being like-for-like.
    let in_arm = |r: &Value, a: &str| r.get("arm").and_then(Value::as_str) == Some(a);
    let all: Vec<&Value> = rows
        .iter()
        .filter(|r| !js::truthy(r.get("error")) && arms.iter().any(|a| in_arm(r, a)))
        .collect();
    let key = |r: &Value, arm: &str| format!("{}|{}|{arm}", text(r, "fixture"), text(r, "seed"));
    let have: HashSet<String> = all.iter().map(|r| key(r, &text(r, "arm"))).collect();
    let ok: Vec<&Value> = all
        .iter()
        .copied()
        .filter(|r| arms.iter().all(|a| have.contains(&key(r, a))))
        .collect();
    let dropped = all.len() - ok.len();
    let select = |arm: &str| {
        ok.iter()
            .copied()
            .filter(|r| in_arm(r, arm))
            .collect::<Vec<_>>()
    };

    by_fixture(out, &ok, arms, &treat);
    overall(out, arms, &select);
    hook_note(out, &select("chisle-hook"));
    significance(out, &treat, &select);
    failures(out, &ok);
    footnotes(out, rows, dropped, treat.len());
}

/// What the compressor did in the hook arm, if it ran.
fn hook_note(out: &mut Out, hook: &[&Value]) {
    if hook.is_empty() {
        return;
    }
    let field_mean = |k: &str| {
        let xs: Vec<f64> = hook.iter().map(|r| js::number_or_zero(r.get(k))).collect();
        mean(&xs)
    };
    out.say(format!(
        "\nchisle-hook: compressor elided {} chars/run across {} outputs/run.",
        to_fixed(field_mean("compressed_chars"), 0),
        to_fixed(field_mean("compress_events"), 2)
    ));
}

/// The cells left out, and why.
fn footnotes(out: &mut Out, rows: &[Value], dropped: usize, treatments: usize) {
    if dropped > 0 {
        let other = if treatments > 1 {
            "every arm"
        } else {
            "the other arm"
        };
        out.say(format!("\n_{dropped} cell(s) excluded: no counterpart in {other}, so the seed sets stay matched._"));
    }
    let errs = rows.iter().filter(|r| js::truthy(r.get("error"))).count();
    if errs > 0 {
        out.say(format!("\n_{errs} cell(s) errored and are excluded._"));
    }
}

fn by_fixture(out: &mut Out, ok: &[&Value], arms: &[String], treat: &[&String]) {
    let mut fixtures: Vec<String> = Vec::new();
    for r in ok {
        let f = text(r, "fixture");
        if !fixtures.contains(&f) {
            fixtures.push(f);
        }
    }
    fixtures.sort();
    out.say("## Hidden-test pass rate by fixture\n");
    let heads: Vec<String> = treat
        .iter()
        .map(|a| {
            if treat.len() > 1 {
                format!("p {a}")
            } else {
                "Fisher p".into()
            }
        })
        .collect();
    out.say(format!(
        "| fixture | {} | {} |",
        arms.join(" | "),
        heads.join(" | ")
    ));
    let rules: Vec<String> = arms.iter().map(|a| rule(a)).collect();
    let p_rules: Vec<String> = heads.iter().map(|h| rule(h)).collect();
    out.say(format!(
        "|---------|{}|{}|",
        rules.join("|"),
        p_rules.join("|")
    ));
    for fx in &fixtures {
        out.say(fixture_row(ok, fx, arms, treat));
    }
}

/// One fixture: each arm's passes, then each treatment's Fisher p against vanilla.
fn fixture_row(ok: &[&Value], fx: &str, arms: &[String], treat: &[&String]) -> String {
    let arm = |a: &str| {
        ok.iter()
            .copied()
            .filter(|r| text(r, "fixture") == fx && r.get("arm").and_then(Value::as_str) == Some(a))
            .collect::<Vec<_>>()
    };
    let v = arm("vanilla");
    // Every arm has cells for a fixture that survived the matching.
    let cell = |c: &[&Value]| format!("{}/{}", num(passed(c)), c.len());
    let cells: Vec<String> = arms.iter().map(|a| cell(&arm(a))).collect();
    let ps: Vec<String> = treat
        .iter()
        .map(|t| {
            let h = arm(t);
            if v.is_empty() {
                return "—".into();
            }
            let (hp, vp) = (passed(&h), passed(&v));
            fmt_p(fisher(hp, h.len() as f64 - hp, vp, v.len() as f64 - vp))
        })
        .collect();
    format!("| {fx} | {} | {} |", cells.join(" | "), ps.join(" | "))
}

fn overall<'a>(out: &mut Out, arms: &[String], select: &dyn Fn(&str) -> Vec<&'a Value>) {
    out.say("\n## Overall\n");
    out.say("| arm | n | passed | pass rate | new files | net LOC | deps added | out tokens | context tokens | turns | $ / run |");
    out.say("|-----|--:|-------:|----------:|----------:|--------:|-----------:|-----------:|---------------:|------:|--------:|");
    for arm in arms {
        let c = select(arm);
        if c.is_empty() {
            continue;
        }
        let m = |f: &dyn Fn(&Value) -> f64| mean(&c.iter().map(|r| f(r)).collect::<Vec<_>>());
        let field_mean = |k: &str| m(&|r| js::number_or_zero(r.get(k)));
        let n = c.len() as f64;
        out.say(format!(
            "| {arm} | {} | {} | {}% | {} | {} | {} | {} | {} | {} | {} |",
            c.len(),
            num(passed(&c)),
            to_fixed(100.0 * passed(&c) / n, 1),
            to_fixed(m(&|r| count(r.get("files_created"))), 2),
            to_fixed(field_mean("loc_delta"), 1),
            to_fixed(m(&|r| count(r.get("deps_added"))), 2),
            to_fixed(field_mean("out_tokens"), 0),
            to_fixed(field_mean("input_tokens"), 0),
            to_fixed(field_mean("turns"), 2),
            to_fixed(field_mean("cost_usd"), 4),
        ));
    }
}

/// Overall significance, stated whether or not it flatters the tool.
fn significance<'a>(out: &mut Out, treat: &[&String], select: &dyn Fn(&str) -> Vec<&'a Value>) {
    let v = select("vanilla");
    // Matched cells mean every treatment arm has cells whenever vanilla does.
    if v.is_empty() || treat.is_empty() {
        return;
    }
    let vp = passed(&v);
    out.say("\n## Significance\n");
    out.say(format!(
        "vanilla {}/{}, 95% CI {}",
        num(vp),
        v.len(),
        ci(vp, v.len() as f64)
    ));
    for t in treat {
        let h = select(t);
        let hp = passed(&h);
        let p = fisher(hp, h.len() as f64 - hp, vp, v.len() as f64 - vp);
        out.say(format!(
            "{} {}/{}, 95% CI {}",
            js::pad(t, 7, false),
            num(hp),
            h.len(),
            ci(hp, h.len() as f64)
        ));
        let versus = if treat.len() > 1 {
            format!(" ({t} vs vanilla)")
        } else {
            String::new()
        };
        let noise = if p < 0.05 {
            ""
        } else {
            " — not separable from noise at this cell count."
        };
        out.say(format!(
            "Fisher exact{versus}, two-sided: p = {}{noise}",
            fmt_p(p)
        ));
    }
}

fn failures(out: &mut Out, ok: &[&Value]) {
    let mut fails: Vec<&Value> = ok
        .iter()
        .copied()
        .filter(|r| !js::truthy(r.get("pass")))
        .collect();
    out.say("\n## Every failed cell\n");
    if fails.is_empty() {
        out.say("_None._");
        return;
    }
    out.say("| arm | fixture | seed | first failing assertion |");
    out.say("|-----|---------|-----:|-------------------------|");
    fails.sort_by(|a, b| {
        js::locale_compare(&text(a, "arm"), &text(b, "arm"))
            .then_with(|| js::locale_compare(&text(a, "fixture"), &text(b, "fixture")))
    });
    for f in fails {
        let output = match f.get("test_output") {
            Some(Value::String(s)) => s.as_str(),
            _ => "",
        };
        let first = output
            .split('\n')
            .next()
            .unwrap_or_default()
            .replace('|', "\\|");
        out.say(format!(
            "| {} | {} | {} | {} |",
            text(f, "arm"),
            text(f, "fixture"),
            text(f, "seed"),
            js::lossy(&js::slice(&first, 100))
        ));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mean_of_nothing_is_zero() {
        assert_eq!(super::mean(&[]).to_bits(), 0.0_f64.to_bits());
        assert!(super::count(Some(&serde_json::json!(true))).is_nan());
    }
}
