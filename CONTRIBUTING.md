# Contributing

Small focused PRs beat big rewrites. Puritanical is a small package. Keep it that way.

## What lives where

| File | Purpose |
|------|---------|
| `skills/puritanical/SKILL.md` | **Behaviour source.** All rules and examples. The activate hook reads it at runtime. |
| `tools/src/rules.rs` | Condensed mirror of the skill for the 7 static-rule agents (OpenCode reuses `AGENTS.md` + `skills/` directly; no generated copy). **Editing SKILL.md alone does not propagate here**, so update the `BODY` too, then regenerate. |
| `hooks/puritanical-activate.js` | SessionStart: reads SKILL.md, writes flag, emits rules |
| `hooks/puritanical-mode-tracker.js` | UserPromptSubmit: `/puritanical` commands, NL detection, per-turn reinforcement |
| `hooks/puritanical-compress-output.js` | PostToolUse: input-side compression (scrub / elide / dedup tiers, savings ledger) |
| `hooks/puritanical-config.js` | Shared flag read/write, mode resolution. Security-sensitive, so test changes carefully. |
| `hooks/puritanical-mode.js` | Shared natural-language mode-directive parser. |
| `pi-extension/index.js` | Pi lifecycle, command, status, and `tool_result` adapter; reuses hook core. |
| `hooks/puritanical-statusline.sh` / `.ps1` | Statusline badge: mode + measured input-side savings |
| `bin/install.js` + `bin/lib/` | Multi-agent installer (`agents.js`, `claude.js`, `uninstall.js`), JSONC-safe settings merge (`settings.js`) |
| `bin/lib/agents.js` + `bin/lib/fence.js` (opencode/hermes paths) | Fenced global ruleset + verbatim skill copies; `PURITANICAL_HOME` redirects home in tests |

## What to edit

**Changing behaviour** → `skills/puritanical/SKILL.md`, **and** the condensed `BODY` in `tools/src/rules.rs`, then `npm run build:rules`. CI checks the copies are in sync with the generator (not with SKILL.md; the mirror is manual, by design).

**Input-side compression** → shared transforms in `hooks/puritanical-compress-output.js`, harness shape/lifecycle in `pi-extension/index.js`. Correctness invariants: allowlist only (never Read/Edit/Write), preserve Pi result metadata, salvage error lines, skip same-turn Pi dedup, keep every tier kill-switchable, never break the tool pipeline.

**Natural language triggers** → `hooks/puritanical-mode.js`; both harnesses consume it.

**Security-sensitive paths** → `hooks/puritanical-config.js` (`safeWriteFlag`, `readFlag`). Symlink-safe, `O_NOFOLLOW`, size-capped. Don't simplify them.

## Tests

```bash
npm test                 # node --test tests/*.js
cd tools && cargo test   # generators and benchmark analysis
```

`tools/` is a Rust crate (`puritanical-tools`) that writes and checks the generated files (rule copies, charts, README art) and runs the benchmark analysis. It needs a Rust toolchain; `tools/rust-toolchain.toml` pins the version CI uses.

`.gitattributes` checks the generated files out with LF endings. A Windows clone made before that keeps CRLF copies, which fail `--check`: commit or stash your work, then check them out again with `git rm -r --cached -q . && git reset --hard`.

Add a test for any hook logic change. Compressor changes go in `tests/test_compress.js`.

## Benchmarks

Numbers in README/docs come from committed raw data. Nothing lands without receipts:

```bash
bash benchmarks/run-live.sh [model] [fresh-raw-dir]             # Claude live run
HARNESS=pi bash benchmarks/run-live.sh [model] [fresh-raw-dir]  # Pi live run
RAW_DIR=<dir> cargo run --quiet --locked --manifest-path tools/Cargo.toml -- aggregate   # tables
node benchmarks/replay-compress.js [claude|pi] [session-dir]     # input replay
```

## PR checklist

- [ ] SKILL.md and the `rules.rs` BODY both updated (if behavior changed) + `npm run build:rules`
- [ ] Hook changes don't break the flag-file security model or the compressor invariants
- [ ] New measurable behavior gets a benchmark task or replay receipt
- [ ] `npm test` and `cd tools && cargo test` pass; `npm run check:rules` and `check:readme-art` clean

## Releases

Pushing a `v*` tag runs `.github/workflows/publish.yml`, which reruns the Node suite and the generated-file checks on the tagged commit, builds `puritanical-tools` for 4 targets, then creates a GitHub release with generated notes, the binaries and `SHA256SUMS`, and records a build provenance attestation for each binary; nothing is published to npm or crates.io.

## Hacktoberfest

Puritanical welcomes Hacktoberfest contributors. Start from an issue labelled `hacktoberfest`, `good first issue` or `help wanted`, and comment before you start so two people don't build the same thing. One issue per PR.

No rewards: Hacktoberfest 2026 no longer counts pull requests, and this is an unfunded, one-person project with no swag or bounties. What you get is a merged change, credit in the changelog, and your name in the repo's history.

What gets merged: a fix, a fixture, a test, a measurement with receipts, a docs correction backed by data. What doesn't: whitespace, typo sweeps across unrelated files, reworded README sections, or anything that adds a dependency; those get labelled `invalid` or `spam`.

If Puritanical saved you tokens, or you enjoyed contributing, a ⭐ on the repo is the best thanks and helps others find it. It's optional and never a condition for review or merge.

Benchmark PRs follow the same rule as everything else: raw cells committed, losses published.

## Reporting bugs

Open an issue. Include: what you typed, what puritanical did, what you expected.
