# Changelog

This file follows the shape of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Dates are release dates. Numbers in brackets are pull requests. Puritanical is a fork of [Ponytail](https://github.com/DietrichGebert/ponytail), taken after upstream v5.3.2 (`09c434a`); for earlier history, see Ponytail's own notes.

## [Unreleased]

### Added

- The results of 2 benchmark runs, with every cell, under `benchmarks/results/`: the 1.0.0 ruleset against the draft before it, and, on 7 November 2026, a pre-release ruleset (`3d145eb`) against Ponytail v5.3.3 (`7801d06`) and no skill, where Ponytail wrote fewer lines and cost less on all 4 tasks; not significant, as with 4 tasks the sign test cannot go below p = 0.125 ([results](benchmarks/results/2026-11-07-head-to-head.md)).

### Fixed

- Claude Code installs follow each commit: `.claude-plugin/plugin.json` no longer declares a version, so Claude Code offers updates for changes made since 1.0.0. `check-versions` refuses a version there (#55).

## [1.0.0] - 2026-11-10

First release. Puritanical guides coding agents to the smallest correct change: read the code and its callers first, then take the first of 7 options that meets the whole request (leave it out, reuse, standard library, platform feature, installed dependency, direct expression, new code). It never strips input validation, security, accessibility, error handling that prevents data loss, or verification. [Changes from Ponytail](README.md#changes-from-ponytail) lists what differs from upstream.

### Added

- The v5 ruleset, which finishes routine changes instead of stopping to ask (#52). Across 470 benchmark cells (47 tasks, Claude Opus 5.5, 5 runs per arm), 100 of 100 surgical cells passed the hidden checks, against 97 for the previous draft ([results](benchmarks/results/2026-11-08-v5-vs-v4.md)). Code size and cost showed no significant difference from that draft, which was about a tenth smaller and cheaper than the one before it (#49).
- A codebase map at session start in Git repositories, so the agent reuses what exists, on by default since #49; `PURITANICAL_MAP=0` turns it off.
- A benchmark harness that runs Puritanical, Ponytail, Superpowers, the Karpathy guidelines and no skill from pinned checkouts, plus behaviour probes for the rules this fork added.
- Installs for Claude Code, Codex and the other hosts the README lists.

### Known limits

- Measured with Claude Opus 5.5 only; other models and hosts are untested.
- The comparison with other projects is not published yet.
- Licence is FSL-1.1-ALv2, not OSI open source; each version becomes Apache-2.0 after 2 years.
- Released on GitHub only; nothing is published to npm.

[Unreleased]: https://github.com/yungSNAT/Puritanical/compare/v1.0.0...main
[1.0.0]: https://github.com/yungSNAT/Puritanical/releases/tag/v1.0.0
