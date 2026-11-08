"""Hermes plugin for Puritanical."""

from __future__ import annotations

import json
import os
import re
from pathlib import Path
from typing import Any, Callable

DEFAULT_MODE = "full"
RUNTIME_MODES = {"off", "lite", "full", "ultra"}
SKILL_COMMANDS = {
    "puritanical-review": "Review the current diff or provided target: bugs, security, load, missing tests, speed, and what to cut.",
    "puritanical-audit": "Audit the whole repo: bugs, security, load, missing tests, speed, and what to cut.",
    "puritanical-debt": "List every deliberate `shortcut:` comment and its upgrade path.",
    "puritanical-help": "Show the Puritanical command reference.",
}

ROOT = Path(__file__).resolve().parent
SKILLS_DIR = ROOT / "skills"
PURITANICAL_SKILL = SKILLS_DIR / "puritanical" / "SKILL.md"

_current_mode = None


def _normalize_runtime_mode(mode: str | None) -> str | None:
    if not isinstance(mode, str):
        return None
    mode = mode.strip().lower()
    return mode if mode in RUNTIME_MODES else None


def _config_dir() -> Path:
    if os.environ.get("XDG_CONFIG_HOME"):
        return Path(os.environ["XDG_CONFIG_HOME"]) / "puritanical"
    if os.name == "nt":
        return Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming")) / "puritanical"
    return Path.home() / ".config" / "puritanical"


def _default_mode() -> str:
    env_mode = _normalize_runtime_mode(os.environ.get("PURITANICAL_DEFAULT_MODE"))
    if env_mode:
        return env_mode
    try:
        data = json.loads((_config_dir() / "config.json").read_text(encoding="utf-8-sig"))
    except (OSError, ValueError, RuntimeError):
        # Missing, unreadable or malformed file, or no home folder to find it in.
        return DEFAULT_MODE
    file_mode = _normalize_runtime_mode(data.get("defaultMode")) if isinstance(data, dict) else None
    return file_mode or DEFAULT_MODE


def _strip_frontmatter(text: str) -> str:
    return re.sub(r"^---[\s\S]*?---\s*", "", text or "", count=1)


def _filter_skill_body_for_mode(body: str, mode: str) -> str:
    effective = _normalize_runtime_mode(mode) or DEFAULT_MODE
    lines = []
    # str.splitlines() drops a trailing line terminator instead of yielding a
    # trailing empty element, so a body ending in "\n" loses that newline on
    # rejoin. re.split keeps it, matching the JS filter's split(/\r?\n/).
    for line in re.split(r"\r?\n", _strip_frontmatter(body)):
        table_label = re.match(r"^\|\s*\*\*(.+?)\*\*\s*\|", line)
        if table_label:
            label_mode = _normalize_runtime_mode(table_label.group(1))
            if label_mode and label_mode != effective:
                continue

        # Require a quoted value: every worked example is `- lite: "..."`. Without
        # this, an ordinary rule bullet that happens to start with a mode word
        # (e.g. "- Full: ...") is silently dropped in every other mode — it looks
        # like a worked example but is really prose meant to survive verbatim.
        # Mirrors the fix in hooks/puritanical-instructions.js (#571).
        example_label = re.match(r'^-\s*([^:\s][^:]*):\s*"', line)
        if example_label:
            label_mode = _normalize_runtime_mode(example_label.group(1))
            if label_mode and label_mode != effective:
                continue

        lines.append(line)
    return "\n".join(lines)


_FALLBACK_SKILL_BODY = "# Puritanical\n\nComplete the requested work with the simplest clear solution that meets its\nrequirements. Correctness, safety, accessibility, maintainability and required\nperformance come before fewer lines, files or tokens.\n\n## Scope and modes\n\nDefault: **full**, until changed or the session ends. Switch with\n`/puritanical lite|full|ultra` or the host's equivalent; `/puritanical off`,\n\"stop puritanical\" or \"normal mode\" turns it off.\n\nFollow the user's scope, project instructions and permissions. A review,\ndiagnosis or plan stays read-only unless changes are requested. Complete\nexplicit requirements without substituting a reduced feature set. A request that states only its goal (\"build me X\") gets the version that does the core job; name the\nextras you left out in one line. Decide routine choices from context; when a reading changes the result, state the one you chose, or ask if it is costly\nto reverse. Use subagents only within explicit user authorisation, keeping its model, task and read-only limits.\nSubagents do not inherit these rules: tell a coding, design or review\nsubagent to apply Puritanical at the current level.\n\n## Understand the change\n\nRead the code you will change, the pattern you will follow and the contracts\nit must keep; start large files with their structure. When you change\nexisting behaviour or a signature, search for the affected symbols and\ninspect their callers first. Check what the change could destroy, expose or\nbreak for the people who use it. For new code, the\nfile you edit and the code you reuse are usually enough; widen only for a\nconcrete dependency or doubt. Check the installed version only for APIs that\nare new, unusual or version-specific.\n\nFor a bug, reproduce it or find a failing check first. If it no longer\nreproduces and the contract holds, say so and make no fix. Separate what\nyou observed from what you inferred. Fix the cause where the contract first\nbreaks, not with a guard that hides it. After two failed fixes, stop and\nrecheck the evidence and the design.\n\n## The ladder\n\nTake the first option that fully meets the requirements:\n\n1. **Required now?** Skip speculative features, options and scaffolding.\n2. **Already in this codebase?** Reuse its code, types and patterns.\n3. **Standard library or platform feature?** Use it when it fits.\n4. **Installed dependency?** Reuse it; add one only for a concrete benefit that justifies its maintenance cost.\n5. **Direct expression?** Use a clear one-liner that stays easy to inspect.\n6. **Otherwise:** write the smallest cohesive implementation.\n\n## Design judgement\n\n- Remove a wrapper that adds no useful contract; keep a helper that adds\n  clarity or removes real duplication. One implementation is not bloat.\n- Prefer fewer concepts; diff size breaks ties after correctness and clarity.\n- Keep every changed line traceable to the request. Remove what your change\n  made unused; report other dead code instead of deleting it. Preserve unrelated work.\n- Choose algorithms by their semantics, failure modes and resource costs;\n  keep the project's optimised stack when a simpler-looking replacement would regress it.\n- Preserve input validation at trust boundaries, security, accessibility,\n  error handling that prevents data loss, and required hardware calibration.\n  In data, money and security paths, reject malformed input or report what was skipped.\n- Mark a deliberate corner cut with a `shortcut:` comment naming the ceiling and\n  upgrade path. Ordinary code needs no justification comment, and nothing needs a Puritanical marker.\n\n## Verification and reporting\n\nUse the project's test tools and required checks. Add a focused test of the\ncontract for changed failure modes when existing coverage is insufficient; a regression test must be one that fails without the fix: run it against the unfixed code or say why you could not. Money and security\ncode can need several cases. There is no one-test ceiling. Never weaken, skip or\ndelete a test or check to make it pass; change an assertion only when it tests\nthe wrong contract, and say why. Reuse valid results. A timeout is inconclusive.\n\nClaim only what this session's evidence shows: name each check and its\nresult, and mark the rest not run. Inspect generated artefacts before calling\nthem usable. Lead with the result, in the user's requested format. Report a small change in a few lines: what\nchanged, the check and any limitation. Give requested reports the detail they\nneed.\n\n## Intensity\n\n| Level | Behaviour |\n| --- | --- |\n| **lite** | Prefer straightforward implementation; mention a simpler alternative when it materially helps. |\n| **full** | Apply the ladder and remove unnecessary complexity within the complete requested change. Default. |\n| **ultra** | Scrutinise complexity more closely while preserving every requirement and required check. |\n"

def _fallback_instructions(mode: str) -> str:
    return f"PURITANICAL MODE ACTIVE — level: {mode}\n\n" + _filter_skill_body_for_mode(_FALLBACK_SKILL_BODY, mode)


def build_injected_context(mode: str | None = None) -> str:
    """Return the mode-filtered Puritanical context injected before LLM turns."""
    effective = _normalize_runtime_mode(mode) or _default_mode()
    if effective == "off":
        return ""
    try:
        body = PURITANICAL_SKILL.read_text(encoding="utf-8")
        return f"PURITANICAL MODE ACTIVE — level: {effective}\n\n{_filter_skill_body_for_mode(body, effective)}"
    except OSError:
        return _fallback_instructions(effective)


def _pre_llm_call(
    session_id: str = "", conversation_history: Any = None, **_: Any
) -> dict[str, str] | None:
    mode = _current_mode or _default_mode()
    # Off is injected too: the ruleset from an earlier turn is replayed every turn,
    # and the /puritanical off reply goes to the user, never to the model.
    context = build_injected_context(mode) or "PURITANICAL MODE OFF"

    # Hermes persists hook context in api_content for cache-stable replay.
    # Re-inject only when the active mode changed or compaction removed it.
    for message in reversed(conversation_history or []):
        if not isinstance(message, dict):
            continue
        api_content = message.get("api_content")
        if not isinstance(api_content, str):
            continue
        matches = re.findall(r"PURITANICAL MODE (?:ACTIVE — level: ([a-z]+)|OFF)", api_content)
        if matches:
            return None if (matches[-1] or "off") == mode else {"context": context}

    # Nothing persisted, so off has nothing to switch off.
    return None if mode == "off" else {"context": context}


def _skill_prompt(command: str, args: str = "") -> str:
    tail = args.strip()
    target = f"\n\nUser arguments: {tail}" if tail else ""
    return (
        f"Load and follow the Hermes plugin skill `puritanical:{command}`. "
        f"{SKILL_COMMANDS[command]}{target}"
    )


def _slash_access_denied(event: Any, gateway: Any, command: str) -> bool:
    if gateway is None or event is None:
        return False
    checker = getattr(gateway, "_check_slash_access", None)
    source = getattr(event, "source", None)
    if checker is None or source is None:
        return False
    try:
        return checker(source, command) is not None
    except Exception:
        return True


def rewrite_gateway_command(event: Any = None, gateway: Any = None, **_: Any) -> dict[str, str] | None:
    """Rewrite authorized gateway /puritanical-* commands into normal agent prompts."""
    text = str(getattr(event, "text", "") or "").strip()
    if not text.startswith("/"):
        return None
    head, _, rest = text[1:].partition(" ")
    command = head.replace("_", "-").lower()
    if command not in SKILL_COMMANDS:
        return None
    if _slash_access_denied(event, gateway, command):
        return None
    return {"action": "rewrite", "text": _skill_prompt(command, rest)}


def _handle_mode_command(raw_args: str) -> str:
    global _current_mode
    arg = (raw_args or "").strip().lower()
    if not arg:
        # Bare /puritanical switches puritanical on, or reports the level when it already is (#639).
        mode = _current_mode or _default_mode()
        if mode != "off":
            return f"Puritanical mode: {mode}. Use `/puritanical lite|full|ultra|off`."
        _current_mode = "full" if _default_mode() == "off" else _default_mode()
        return f"Puritanical mode set to {_current_mode}."
    mode = _normalize_runtime_mode(arg)
    if not mode:
        return "Usage: /puritanical [lite|full|ultra|off]"
    _current_mode = mode
    return f"Puritanical mode set to {mode}."


def _make_skill_command_handler(ctx: Any, command: str) -> Callable[[str], str]:
    def handler(raw_args: str) -> str:
        prompt = _skill_prompt(command, raw_args or "")
        injected = False
        try:
            injected = bool(ctx.inject_message(prompt))
        except Exception:
            injected = False
        if injected:
            return f"Queued `{command}` for the agent."
        return prompt

    return handler


def register(ctx: Any) -> None:
    """Register Puritanical hooks, skills, and slash commands with Hermes."""
    for child in sorted(SKILLS_DIR.iterdir() if SKILLS_DIR.exists() else []):
        skill_md = child / "SKILL.md"
        if child.is_dir() and skill_md.exists():
            ctx.register_skill(child.name, skill_md)

    ctx.register_hook("pre_llm_call", _pre_llm_call)
    ctx.register_hook("pre_gateway_dispatch", rewrite_gateway_command)

    ctx.register_command(
        "puritanical",
        _handle_mode_command,
        description="Set the Puritanical level: lite, full, ultra or off.",
        args_hint="[lite|full|ultra|off]",
    )
    for command, description in SKILL_COMMANDS.items():
        ctx.register_command(
            command,
            _make_skill_command_handler(ctx, command),
            description=description,
            args_hint="[target or notes]",
        )
