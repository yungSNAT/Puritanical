---
name: puritanical-help
metadata:
  author: Jay Pokale
description: >
  Quick-reference card for Puritanical commands. One-shot
  display, not a persistent mode. Use when the user says "puritanical help", "how do I
  use puritanical", "what puritanical commands", or invokes /puritanical-help.
---

# Puritanical quick reference

**What it is:** maximum-efficiency dev mode. Plain, answer-first Laconic prose + YAGNI-first code, always on together.

## Commands

| Command | Effect |
|---------|--------|
| `/puritanical` | Activate |
| `/puritanical-audit [path]` | Audit a diff/file/repo for both code bloat AND prose verbosity |
| `/puritanical-review` | Review the current diff for over-engineering |
| `stop puritanical` / `normal mode` | Deactivate |

A whole instruction also works: "puritanical mode", "activate puritanical", "stop puritanical".

## The code ladder

YAGNI → reuse → stdlib → native → installed dep → one line → minimum code.

## Statusline

`[PURITANICAL]` is shown while active. Configure via
`hooks/puritanical-statusline.sh` (or `.ps1` on Windows).

## Never minimal about

Input validation, error handling that prevents data loss, security,
accessibility, anything you explicitly asked for.
