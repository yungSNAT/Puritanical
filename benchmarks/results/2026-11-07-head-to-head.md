# Head to head: a pre-release Puritanical, Ponytail and no skill, 4 tasks

*Run 7 November 2026. Claude Code 2.1.318, `claude-opus-5-5`, 4 tasks, 5 runs per task and arm, 60 sessions.*

**Result:** Ponytail wrote fewer lines and cost less than Puritanical on all 4 tasks. Puritanical wrote fewer lines than no skill on all 4 and cost less on 3. Every cell passed its check, and every judged cell scored complete.

This run measured Puritanical at `3d145eb`, before the rulesets of #49 and #52. **Nothing has measured 1.0.0 against Ponytail or no skill.** Do not read these figures as a result for 1.0.0.

## Setup

- **Arms:**
  - `baseline`: no skill.
  - `puritanical`: [`3d145eb`](https://github.com/yungSNAT/Puritanical/commit/3d145eb6f7b60ca268afb70883eb99336aadf1c0), the ruleset before 1.0.0. The codebase map was off by default then.
  - `ponytail`: [DietrichGebert/ponytail `7801d06`](https://github.com/DietrichGebert/ponytail/commit/7801d061243d7b753bad0e0b3c057cd250b11755), the v5.3.3 release commit.
- **Harness:** [`benchmarks/agentic/`](../agentic/) at `3d145eb`. Each cell is a fresh headless Claude Code session that loads only its own arm's plugin, as [Head to head](../agentic/README.md#head-to-head) describes.
- **Tasks:** `csv-sum`, `open-dataclass`, `tmpl-be-count` and `vibe-todo`, defined in [`tasks.py`](../agentic/tasks.py). `tmpl-be-count` runs in [full-stack-fastapi-template](https://github.com/fastapi/full-stack-fastapi-template) at `cd83fc1`.
- **Judges:** completeness, `claude-opus-5-5`, on the 45 cells of the 3 open tasks (`csv-sum` has a deterministic check). Over-engineering, `claude-sonnet-4-6`, on all 60 cells. Both ran through the Claude Code CLI, which has no temperature setting.
- **Cost:** US$9.46 at list price, as Claude Code reported it. One `csv-sum` Ponytail cell was stopped by an outer time limit and rerun with `--resume`; the stopped attempt, about US$0.11, is not in that total.

## Results

Means over 5 runs. Lines are non-blank source lines the agent wrote, comments included and tests excluded (`total_loc`). Cost is US$ per session.

| Task | No skill: lines | Puritanical: lines | Ponytail: lines | No skill: US$ | Puritanical: US$ | Ponytail: US$ |
| --- | --- | --- | --- | --- | --- | --- |
| `csv-sum` | 65.6 | 31.2 | 26.6 | 0.146 | 0.154 | 0.116 |
| `open-dataclass` | 76.8 | 55.4 | 48.6 | 0.118 | 0.094 | 0.081 |
| `tmpl-be-count` | 25.8 | 24.8 | 11.8 | 0.289 | 0.287 | 0.154 |
| `vibe-todo` | 244.4 | 94.6 | 54.8 | 0.212 | 0.134 | 0.106 |
| **Sum of the 4** | **412.6** | **206.0** | **141.8** | **0.766** | **0.669** | **0.458** |

- **Correct and safe:** 20 of 20 cells in every arm. Only `csv-sum` has a hidden correctness and safety check. `tmpl-be-count` passes when it changes code, `vibe-todo` when it compiles, and `open-dataclass` is scored on size only.
- **Completeness:** every one of the 45 judged cells scored 3 of 3 in all 3 arms, so the judge cannot separate the arms here.
- **Over-engineering** (0 is minimal, 3 over-built): mean 0.80 for no skill, 0.40 for Puritanical and 0.15 for Ponytail.
- **Paired comparison** (`compare.py`, below): Ponytail wins on lines (`loc`) and cost on 4 of 4 tasks against Puritanical. Puritanical wins on lines on 4 of 4 tasks against no skill, and on cost on 3 of 4. With 4 tasks the sign test cannot go below p = 0.125, so none of these differences is significant.

## Limits

- **Pre-release ruleset:** the `puritanical` arm is `3d145eb`, not 1.0.0.
- **Small:** 4 tasks and 5 runs. This is not the full head-to-head that the [analysis plan](../agentic/README.md#analysis-plan) fixes (47 tasks, 5 arms), so it supports no claim under that plan. Its 4 tasks are the plan's pilot set, and the plan does not count pilot cells. It is published as descriptive evidence.
- **Checks:** correctness and completeness are at their ceiling for every arm here. They show no arm failing; they do not show what the extra lines buy.
- **Scope:** one model, one host (Claude Code), one date. Superpowers and the Karpathy guidelines were not in this run.

## Data and reproduction

[`2026-11-07-head-to-head/`](2026-11-07-head-to-head/) holds every cell, ties included:

- `results.json`: one row per session, as the harness wrote it;
- `completeness.json` and `judge.json`: each judge's score per cell;
- `sources.json`: the commits run.

Local paths, judge rubrics and judges' free-text reasons are left out. The rubrics are in `complete.py` and `judge.py`.

Recheck the comparison from the published cells, with no API calls:

```bash
cd benchmarks/agentic
python compare.py ../results/2026-11-07-head-to-head --a puritanical --b baseline,ponytail
```

To rerun, check out the commits in `sources.json` and follow [Head to head](../agentic/README.md#head-to-head), with `--task csv-sum,open-dataclass,tmpl-be-count,vibe-todo --arms baseline,puritanical,ponytail --models opus --runs 5`.
