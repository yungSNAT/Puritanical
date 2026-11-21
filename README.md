<h1 align="center">
  <picture>
    <source media="(max-width: 600px) and (prefers-color-scheme: light)" srcset=".github/assets/banner-phone-light.svg">
    <source media="(max-width: 600px)" srcset=".github/assets/banner-phone.svg">
    <source media="(prefers-color-scheme: light)" srcset=".github/assets/banner-light.svg">
    <img src=".github/assets/banner.svg" width="100%" alt="Puritanical. No accretions. The smallest correct change, verified.">
  </picture>
</h1>

<div align="center">

[![Tests](https://img.shields.io/github/actions/workflow/status/yungSNAT/Puritanical/test.yml?branch=main&label=tests)](https://github.com/yungSNAT/Puritanical/actions/workflows/test.yml) [![Codecov](https://codecov.io/gh/yungSNAT/Puritanical/graph/badge.svg)](https://codecov.io/gh/yungSNAT/Puritanical) [![CodeQL · no workflow](.github/assets/badges/codeql.svg)](#project-checks) [![OSV-Scanner · no workflow](.github/assets/badges/osv-scanner.svg)](#project-checks) [![SonarCloud · awaiting analysis](.github/assets/badges/sonarcloud.svg)](#project-checks) [![Codacy · awaiting analysis](.github/assets/badges/codacy.svg)](#project-checks)<br>
[![OpenSSF Scorecard · awaiting assessment](.github/assets/badges/scorecard.svg)](#project-checks)<br>
[![Code licence · MIT](.github/assets/badges/licence.svg)](#licence) [![GitHub release](https://img.shields.io/github/v/release/yungSNAT/Puritanical?color=66023C&labelColor=000000&style=flat)](https://github.com/yungSNAT/Puritanical/releases) [![Fork of Chisle](.github/assets/badges/fork-of.svg)](https://github.com/JayPokale/Chisle)

[Install](#install) · [Use](#use) · [The rules](#the-rules) · [Benchmarks](#benchmarks) · [Development](#development) · [Credits](#credits) · [Licence](#licence)

</div>

Puritanical guides coding agents towards the smallest correct change. It asks the agent to read the code and its callers, check whether a reported bug still exists and preserve the behaviour callers rely on. The name comes from the Puritan practice of stripping away what scripture did not warrant.

## Install

For Codex, clone the fork and install its rules with Node.js 18 or later.

```powershell
git clone https://github.com/yungSNAT/Puritanical.git
Set-Location Puritanical
node bin/install.js --only codex
```

The installer adds a marked block to your personal `~/.codex/AGENTS.md`. If the block already exists, add `--force` to refresh it. Start a new chat to load the rules. To remove the block, run `node bin/install.js --uninstall --only codex`.

For one Claude Code session, run `claude --plugin-dir .` from the clone. For a project that reads `AGENTS.md`, you can also copy the [project rules](AGENTS.md) into its existing instructions.

## Use

Ask the agent to ‘fix pagination dropping the first item on page 2’. If the reported case and nearby page boundaries already work, it should explain what it checked and leave the files alone. If the bug persists, it should fix the calculation shared by the affected callers. Review the evidence and the diff.

The Claude Code plugin keeps its upstream command names. Use `/chisle off` to pause it and `/chisle` to resume. Its review skill looks for unnecessary code in a diff. [Commands and configuration](docs/usage.md#usage) · [Review skill](skills/chisle-review/SKILL.md)

## The rules

1. Check the report against the current implementation and its callers.
2. Reuse existing code, the standard library, platform features or installed dependencies where they meet the request. Write new code when the simpler options fall short.
3. Follow the caller’s error policy. A best-effort CSV import can keep valid rows and report skips. An atomic inventory update must reject an invalid batch before changing any stock.
4. Keep input validation, security, accessibility and error handling that prevents data loss. Keep the explanation short enough to read and complete enough to act on.

[Full coding rules](skills/chisle/SKILL.md)

## Benchmarks

On 21 November 2026, GPT-6.2 Astra answered 8 coding tasks twice with each ruleset. Puritanical passed all 16 attempts and used the fewest total tokens. It used the most output tokens.

![Passed attempts out of 16 on GPT-6.2 Astra. Puritanical 16, Chisle 15, Ponytail 14.](.github/assets/astra-results.svg)

| Ruleset | Passed | Input tokens | Output tokens | Total tokens |
| --- | ---: | ---: | ---: | ---: |
| **Puritanical** | **16/16** | 200,652 | 4,306 | **204,958** |
| Chisle | 15/16 | 202,220 | 4,281 | 206,501 |
| Ponytail | 14/16 | 201,004 | 4,273 | 205,277 |

The difference in passes came from the already-fixed pagination task. Puritanical left the files alone both times. Chisle added an unnecessary test once and Ponytail twice. All 3 repaired the paired task where pagination still failed. The 8 tasks were deliberately chosen and the model had no tools. [Method, per-task results and replay](benchmarks/results/2026-11-21-astra/README.md) · [All 48 answers and token counts](benchmarks/results/2026-11-21-astra/receipts.json)

## Development

Edit the [canonical skill](skills/chisle/SKILL.md), then run `npm run build:rules` to regenerate its copies. Run the following checks from the clone.

```powershell
npm test
npm run check:rules
npm run check:chart
npm run check:samples
```

### Project checks

The test workflow covers Node.js 18, 20 and 22. Codecov has no coverage upload for this fork yet. It has no CodeQL or OSV-Scanner workflow. The SonarCloud, Codacy and OpenSSF badges are awaiting results for the replacement repository. [Test workflow](.github/workflows/test.yml) · [Workflow history](https://github.com/yungSNAT/Puritanical/actions)

## Credits

Ryan Duguid maintains Puritanical. Jay Pokale wrote the upstream plugin. The original Puritanical banner uses Cascadia Mono and Besley lettering. [Upstream and third-party credits](NOTICE)

## Licence

The code uses the [MIT licence](LICENSE). The restored banner artwork is © 2026 Ryan Duguid and carries its original [Apache 2.0 licence](.github/assets/LICENSE-banner.txt). Its lettering uses glyphs distributed under SIL OFL 1.1.

Scripture quoted by permission. Quotations designated (NET) are from the NET Bible® copyright ©1996, 2019 by Biblical Studies Press, L.L.C. All rights reserved.

---

<p align="center"><sub><i>'Do not add a thing to what I command you nor subtract from it.'</i> Deuteronomy 4:2 (<a href="https://netbible.org/">NET</a>)<br>Maintained by <a href="https://github.com/yungSNAT">yungSNAT</a>. <i>Ad gloriam eius, non meam.</i></sub></p>
