// SPDX-License-Identifier: Apache-2.0 AND MIT

//! The panels' English strings.

use super::Lang;

fn en_words(a: usize, b: usize) -> String {
    format!("{a} words → {b} words")
}
fn en_hit(k: usize, a: &str) -> String {
    format!("✓ rung {k}: {a}")
}

pub(super) const EN: Lang = Lang {
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
    strip_title: "Chisle is free. Its benchmarks aren't.",
    strip_sub: "Saved you tokens? A coffee pays for the next benchmark run.",
    free: "Chisle is free.",
    not_free: "Its benchmarks aren't.",
    body1: "Every benchmark run costs real API money.",
    body2: "Saved you tokens? A coffee pays for the next run.",
    button: "☕ Buy me a coffee",
};
