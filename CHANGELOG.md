# Changelog

This file follows the shape of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Dates are release dates. Numbers in brackets are pull requests. Puritanical is a fork of [Ponytail](https://github.com/DietrichGebert/ponytail), taken after upstream v5.3.2 (`09c434a`); for earlier history, see Ponytail's own notes.

## [Unreleased]

### Added

- The results of 4 head-to-head rounds of 1.0.0 against Ponytail v5.3.3 and no skill, run on 14 November 2026, with every cell of the largest. With Claude Opus 5.5 on 20 tasks, 1.0.0 passed both checks in 199 of 200 cells, Ponytail in 192 and no skill in 180; Ponytail wrote fewer lines and cost less, and all of 1.0.0's lead comes from one task this fork added ([results](benchmarks/results/2026-11-14-v1.0.0-head-to-head.md)).

### Fixed

- Since #62 each Hermes session keeps its own level; the message Hermes shows after install now says so, instead of 'process-local', and lists `/puritanical default <level>` (#72).
- On a Hermes version that passes no session to plugin commands, `/puritanical default <level>` now reaches new sessions instead of pinning every session at the old default, and a bare `/puritanical` resumes every chat that 'stop puritanical' or 'normal mode' switched off, as other commands on such a version already apply to every chat (#68).

## [1.0.1] - 2026-11-13

### Added

- The results of 2 benchmark runs, with every cell, under `benchmarks/results/`: the 1.0.0 ruleset against the draft before it, and, on 7 November 2026, a pre-release ruleset (`3d145eb`) against Ponytail v5.3.3 (`7801d06`) and no skill, where Ponytail wrote fewer lines and cost less on all 4 tasks; not significant, as with 4 tasks the sign test cannot go below p = 0.125 ([results](benchmarks/results/2026-11-07-head-to-head.md)).

### Fixed

- Claude Code installs follow each commit: `.claude-plugin/plugin.json` no longer declares a version, so Claude Code offers updates for changes made since 1.0.0. `check-versions` refuses a version there (#55).
- Codex installs receive this release: Codex refreshes a plugin only when its manifest version changes, so installs made since 1.0.0 kept the code of their install day.
- The codebase map labels the names it reads from project files as data, not instructions, and leaves out any name or folder name over 40 characters, so a file someone else wrote cannot put a long instruction into the agent's context. Outside Git, a folder that cannot be read is skipped instead of losing the whole map (#60, #64).
- INSTALL points OpenCode at `<checkout>/.opencode/plugins`; the relative path it gave resolved in the user's project (#58).
- OpenCode switches off on a message that is only 'stop puritanical' or 'normal mode', as the other hosts do (#61).
- Hermes honours 'stop puritanical' and 'normal mode', and accepts `/puritanical default <level>`, which sets the level new sessions start in (#62).

### Changed

- Hermes keeps the level per session: on a gateway, `/puritanical off` in one chat no longer switches every chat off. On a Hermes version that passes no session to plugin commands, the level stays process-wide as before (#62).

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

[Unreleased]: https://github.com/yungSNAT/Puritanical/compare/v1.0.1...main
[1.0.1]: https://github.com/yungSNAT/Puritanical/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/yungSNAT/Puritanical/releases/tag/v1.0.0
