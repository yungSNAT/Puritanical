# Puritanical, Chisle and Ponytail on GPT-6.2 Astra

Puritanical passed 16 of 16 attempts, Chisle 15 and Ponytail 14. All three produced working application code on every attempt. The three failed verdicts came from adding a regression test for an already-fixed bug. The original fixture requires leaving the repository unchanged when the reported and related cases already work.

| Ruleset | Passed | Input tokens | Cached input, included | Output tokens | Input + output |
| --- | ---: | ---: | ---: | ---: | ---: |
| Puritanical | 16/16 | 200,652 | 86,144 | 4,306 | 204,958 |
| Chisle | 15/16 | 202,220 | 72,832 | 4,281 | 206,501 |
| Ponytail | 14/16 | 201,004 | 82,304 | 4,273 | 205,277 |

Across all 16 answers, Puritanical used 1,543 fewer total tokens than Chisle (0.75%) and 319 fewer than Ponytail (0.16%). Its output was 25 tokens longer than Chisle’s and 33 longer than Ponytail’s. These totals include failures and shared CLI input. They measure token use under the recorded settings. No monetary cost was estimated.

## Where the rules mattered

The fork asks the model to check a reported bug against the current implementation and its callers. On the already-fixed pagination task it returned an empty change set both times. Chisle added a test once and Ponytail twice. On the paired task, where the apparent fix still failed on other inputs, all six answers repaired the bug.

The fork also asks the model to preserve the caller’s error policy. All six CSV answers summed the valid records. Both Puritanical answers also warned about the malformed record. The four comparison answers skipped it silently. Reporting skips was a secondary observation, separate from the primary pass. All six inventory answers rejected an invalid batch without applying a partial update.

| Task | Puritanical passes / output tokens | Chisle passes / output tokens | Ponytail passes / output tokens |
| --- | ---: | ---: | ---: |
| Already-fixed pagination | 2/2 · 18 | 1/2 · 118 | 0/2 · 178 |
| Partly fixed pagination | 2/2 · 110 | 2/2 · 326 | 2/2 · 307 |
| Sum an imperfect CSV | 2/2 · 1,404 | 2/2 · 1,054 | 2/2 · 987 |
| Apply an atomic inventory update | 2/2 · 916 | 2/2 · 914 | 2/2 · 918 |
| Reuse the currency formatter | 2/2 · 208 | 2/2 · 226 | 2/2 · 314 |
| Rename a field throughout a project | 2/2 · 678 | 2/2 · 678 | 2/2 · 678 |
| Fix negative amounts across display paths | 2/2 · 813 | 2/2 · 815 | 2/2 · 723 |
| Deduplicate IDs in first-seen order | 2/2 · 159 | 2/2 · 150 | 2/2 · 168 |

## What this comparison covers

The two changes came from earlier Puritanical experiments that exposed unnecessary edits to working code and conflicting error policies for partial and atomic operations. We retained two diagnostic pairs and added two fixtures from each comparison project. The 8 public tasks were selected deliberately, with 2 fresh responses per task and ruleset. Eight of the 24 task-ruleset pairs returned byte-identical answers across their repeats. The differences here do not establish a general ranking on unseen work.

All 48 requests used GPT-6.2 Astra at medium effort. Each received the same task, starter files and JSON answer contract, with only the full coding skill body varied. Each completed in one turn without tool calls or retries. This tests rules applied to supplied code. Interactive repository exploration, native plugin hooks and shell-output compression were outside the run. [Pinned sources and invocation](SOURCES.md) · [Registered tasks, hashes and order](registration.json)

## Replay

From this directory on Linux with Python 3, Node.js and bubblewrap installed, run:

```sh
python3 replay.py
```

The replay reconstructs all 48 prompts, checks the saved answers against their hashes, runs the original checks in disposable network-isolated filesystems and reports token sums. Sixteen positive and negative fixture controls passed before generation. The saved JSON contains every answer, verdict and CLI usage count. [Receipts](receipts.json)
