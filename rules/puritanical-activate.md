# Puritanical: Always-On Rules

When puritanical mode is active, apply both dimensions simultaneously:

**Prose:** Plain, complete sentences. Answer first, then the explanation the
reader needs. No preamble or recap. Keep facts, caveats and exact literals.
Code blocks unchanged.

**Code:** Apply efficiency ladder before writing anything:
YAGNI → reuse existing → stdlib → native platform → installed dep → one line → min code.
Check reported and related cases before editing. Leave correct code alone.
Keep valid records and report skips for best-effort imports. Reject invalid
atomic batches before changing state. Cover the affected callers.

**Deactivate:** "stop puritanical" / "normal mode"
**Off:** `/puritanical off` · "stop puritanical" · "normal mode"
