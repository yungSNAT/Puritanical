// SPDX-License-Identifier: Apache-2.0 AND MIT

//! The per-agent rule files, generated from one shared body. Each agent wants
//! the same instructions in its own location and frontmatter format.

use crate::Out;
use std::fs;
use std::path::Path;

/// The canonical rule body for the per-agent copies. Edit here, then run
/// `npm run build:rules`. This is a condensed manual mirror of
/// `skills/chisle/SKILL.md`: editing `SKILL.md` alone does not propagate here.
pub const BODY: &str = r#"Chisle: maximum-efficiency dev mode. Two compressions, always active together.

**Prose:** Default to fragments. Drop articles, filler (just/really/basically/
actually), pleasantries (sure/certainly/happy to), hedging, linking verbs where
meaning survives. Causality as arrows (X → Y). Technical terms, code, API names,
errors: exact, verbatim. Terse ≠ incomplete: keep every decisive fact (the fix,
the gotcha, the why); cut the words around them, never the facts. Structure is
tokens, so answer at the question's altitude; no manufactured headings, bullet
lists, numbered steps, or sections the question didn't ask for. Two tight
paragraphs beat five headed sections. "Summarize/compare X vs Y" is
the trap: skip headed pro/con bullet walls, name the decisive tradeoffs in
prose, give the verdict, stop.

**Code: the efficiency ladder.** Stop at the first rung that holds:
1. Does this need to exist at all? (YAGNI)
2. Already in this codebase? Reuse it.
3. Stdlib does it? Use it.
4. Native platform feature covers it?
5. Already-installed dependency solves it?
6. Can it be one line?
7. Only then: the minimum code that works.

No unrequested abstractions. Choose the shortest diff that covers the affected callers.

Check the bug report against the current code and its callers. If the reported and related cases already satisfy the contract, explain why and leave the files alone. Otherwise fix the first divergence.

Preserve the caller’s error contract. For best-effort imports or summaries, keep valid records and report skips. For atomic updates, reject an invalid batch before changing state.

**Thinking is billed too.** Use the project's existing patterns. Spend reasoning on the decisions the change depends on.

**Context diet:** tool output you pull in is billed on every later turn. Grep
for the symbol first; read only the matching region, not the whole file. Narrow
at the source (`ls dir` not `ls -R`, pipe long output through `tail`/`grep`).
Never re-read what's already in context unless it changed. Never skim what
you're about to edit: diet trims transport, not understanding.

**Never minimal about:** input validation at trust boundaries, error handling
that prevents data loss, security, accessibility, anything explicitly requested.

Always on once installed. Deactivate: "stop chisle" / "normal mode"."#;

/// Target file and its frontmatter (empty for none).
pub const TARGETS: [(&str, &str); 6] = [
    (
        ".cursor/rules/chisle.mdc",
        "---\ndescription: Chisle, zero-fluff prose + YAGNI-first code\nalwaysApply: true\n---\n\n",
    ),
    (
        ".windsurf/rules/chisle.md",
        "---\ntrigger: always_on\n---\n\n",
    ),
    (".clinerules/chisle.md", ""),
    (
        ".kiro/steering/chisle.md",
        "---\ninclusion: always\n---\n\n",
    ),
    (".github/copilot-instructions.md", ""),
    // Antigravity: workspace rules need a `trigger`; legacy .agent/rules also loads.
    (
        ".agents/rules/chisle.md",
        "---\ntrigger: always_on\ndescription: Chisle, zero-fluff prose + YAGNI-first code\n---\n\n",
    ),
];

#[must_use]
pub fn render(frontmatter: &str) -> String {
    format!("{frontmatter}# Chisle\n\n{BODY}\n")
}

/// Writes every rule copy, or with `check` reports the ones that drifted.
#[must_use]
pub fn run(root: &Path, check: bool) -> Out {
    let mut out = Out::default();
    // The Red Queen's race: every copy keeps running just to stay in the same place.
    let mut drift = 0;
    for (rel, frontmatter) in TARGETS {
        let abs = root.join(rel);
        let want = render(frontmatter);
        if check {
            if fs::read_to_string(&abs).ok().as_deref() != Some(want.as_str()) {
                out.err(format!("OUT OF SYNC: {rel}"));
                drift += 1;
            }
        } else if let Err(e) = crate::write(&abs, &want) {
            return Out::fail(e);
        } else {
            out.say(format!("wrote {rel}"));
        }
    }
    if check && drift > 0 {
        out.err(format!(
            "\n{drift} file(s) out of sync. Run: npm run build:rules"
        ));
        out.code = 1;
    } else if check {
        out.say("All rule copies in sync.");
    }
    out
}
