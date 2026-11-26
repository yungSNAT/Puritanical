---
name: puritanical-audit
metadata:
  author: Jay Pokale
description: >
  One-shot efficiency audit of a file, diff, or whole repo across BOTH axes at
  once: over-engineered code (reinvented stdlib, needless abstractions,
  speculative config) AND bloated prose (preamble, recaps, padding, verbose
  comments, redundant doc sections). Neither a pure code-minimizer nor a pure prose
  compressor does both in one pass. That's the point. Ranked report, biggest
  saving first; changes nothing. Use when the user says "puritanical audit", "/puritanical-audit",
  "audit this for bloat", "what can I cut", "review this PR for over-engineering
  and verbosity".
---

# Puritanical Audit

Scan the target (a diff, a file, or the repo tree) and report what to cut, on
both axes. One-shot. Read-only: never edit, never write a flag, never apply fixes.

## Scope

- No argument → audit the current `git diff` (staged + unstaged). Empty diff → audit `HEAD~1..HEAD`.
- A path → audit that file or directory.
- "repo" / "whole repo" → walk the tree (skip vendored/generated/`node_modules`/`dist`/lockfiles).

## What to flag

**Code (the YAGNI axis):**
- Reinvented stdlib (hand-rolled debounce, deep-clone, groupBy, retry loop, date math)
- Abstraction with one implementation (interface/factory/wrapper for a single case)
- New dependency for what a few lines or an installed dep already covers
- Config/option/flag that never varies
- Speculative "for later" scaffolding with no current caller
- Verbose code where a native platform feature (CSS, DB constraint, `<input type>`) does it

**Prose (judged by Laconic's rules), the half a code-only auditor misses:**
- Comments that restate the code (`i += 1  // increment i`)
- Preamble, recaps, closing offers and repeated content in docs, docstrings and READMEs
- Filler, hedging the evidence does not need, puffery and other AI writing patterns
- Multi-paragraph explanations where one plain sentence carries the meaning
- Decorative tables/emoji/headings that add tokens, not information
- Dead prose: TODO graveyards, stale "see also" links, obsolete sections

## What NOT to flag

Input validation at trust boundaries, error handling that prevents data loss,
security, accessibility, deliberate `// puritanical:` / `// ponytail:` shortcuts already
documented, or domain comments that explain *why* (not *what*). Complete
sentences, articles and needed caveats are not bloat: the lean replacement is
plain, complete prose.

## Output

One ranked list, biggest cut first. One finding per line. No preamble, no praise.

```
path:line  [code|prose]  <what's bloated> → <the lean replacement>. (~N lines/tokens)
```

End with a two-line summary:

```
N findings: X code, Y prose. Est. removable: ~A lines code, ~B lines prose.
Biggest win: <the single highest-impact cut>.
```

Be honest about uncertainty: mark a finding `(check)` if cutting it might lose
behavior you can't verify from the snippet. Lean toward fewer, high-confidence
findings over a long speculative list.
