# FAQ

**Will it golf my code into clever one-liners I'll hate at 3am?**
No. The rule is *necessary*, not *fewest characters*. Boring over clever. Deletion beats addition; obfuscation isn't deletion.

**Does it ever cut corners on safety?**
Never. Input validation, error handling that prevents data loss, security, and accessibility are explicitly off the table. It's lazy about solutions, not about reading the problem.

**It made my answer terse and dropped something I needed!**
File an issue. That's a bug, not the design. Terse ≠ incomplete: keep the fix, cut the fluff. If it dropped the fix, it failed its own rules and we want to know.

**Does it work outside Claude Code?**
Yes. It ships to Pi, OpenCode, Copilot CLI, Hermes, Cursor, Windsurf, Cline, Kiro, Antigravity, Codex, Gemini, and Copilot. Pi and Claude Code get live mode switching, status badge, and input compression; OpenCode and Copilot CLI get input compression without the live toggle; the static-rule agents get the always-on ruleset, plus MCP output compression via `npx chisle mcp wrap`. See [agent portability](agent-portability.md).

**Why "Chisle"?**
CHISLE is a demolition charge. Your token bill is the building. The only thing it detonates is verbosity.
