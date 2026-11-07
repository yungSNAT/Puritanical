#!/usr/bin/env node
// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: FSL-1.1-ALv2

// Shared Puritanical instruction builder for Claude hooks and Pi extension.

const fs = require('fs');
const path = require('path');
const { DEFAULT_MODE, normalizeMode, normalizePersistedMode } = require('./puritanical-config');

const INDEPENDENT_MODES = new Set(['review']);
const SKILL_PATH = path.join(__dirname, '..', 'skills', 'puritanical', 'SKILL.md');

function filterSkillBodyForMode(body, mode) {
  const effectiveMode = normalizeMode(mode) || DEFAULT_MODE;
  const withoutFrontmatter = String(body || '').replace(/^---[\s\S]*?---\s*/, '');

  // Only the intensity table rows and worked examples are mode-specific, and
  // both are keyed by a mode name (lite/full/ultra). A bullet whose label is
  // not a mode — e.g. "No unrequested abstractions: ..." — is a normal rule
  // and must be kept verbatim.
  return withoutFrontmatter
    .split(/\r?\n/)
    .filter((line) => {
      const tableLabel = line.match(/^\|\s*\*\*(.+?)\*\*\s*\|/);
      if (tableLabel) {
        const labelMode = normalizeMode(tableLabel[1].trim());
        if (labelMode) return labelMode === effectiveMode;
      }

      // Require a quoted value: every worked example is `- lite: "..."`. Without
      // this, an ordinary rule bullet that happens to start with a mode word
      // (e.g. "- Full: ...") is silently dropped in every other mode — it looks
      // like a worked example but is really prose meant to survive verbatim.
      const exampleLabel = line.match(/^-\s*([^:\s][^:]*):\s*"/);
      if (exampleLabel) {
        const labelMode = normalizeMode(exampleLabel[1].trim());
        if (labelMode) return labelMode === effectiveMode;
      }

      return true;
    })
    .join('\n');
}

const FALLBACK_SKILL_BODY = "# Puritanical\n\nComplete the requested work with the simplest clear solution that meets its\nrequirements. Correctness, safety, accessibility, maintainability and required\nperformance come before fewer lines, files or tokens.\n\n## Scope and modes\n\nDefault: **full**, until changed or the session ends. Switch with\n`/puritanical lite|full|ultra` or the host's equivalent; `/puritanical off`,\n\"stop puritanical\" or \"normal mode\" turns it off.\n\nFollow the user's scope, project instructions and permissions. A review,\ndiagnosis or plan stays read-only unless changes are requested. Complete\nexplicit requirements without substituting a reduced feature set. A broad\nrequest (\"build me X\") gets the version that does the core job; name the\nextras you left out in one line. Decide routine choices from context; when a\nreading changes the result, state the one you chose, or ask if it is costly\nto reverse. Use subagents only within the user's authorisation and limits.\nSubagents do not inherit these rules: tell a coding, design or review\nsubagent to apply Puritanical at the current level.\n\n## Understand the change\n\nRead the code you will change, the pattern you will follow and the contracts\nit must keep; start large files with their structure. When you change\nexisting behaviour or a signature, search for the affected symbols and\ninspect their callers first. Check what the change could destroy, expose or\nbreak for the people who use it. For new code, the\nfile you edit and the code you reuse are usually enough; widen only for a\nconcrete dependency or doubt. Check the installed version only for APIs that\nare new, unusual or version-specific.\n\nFor a bug, reproduce it or find a failing check first. If it no longer\nreproduces and the contract holds, say so and make no fix. Separate what\nyou observed from what you inferred. Fix the cause where the contract first\nbreaks, not with a guard that hides it. After two failed fixes, stop and\nrecheck the evidence and the design.\n\n## The ladder\n\nTake the first option that fully meets the requirements:\n\n1. **Required now?** Skip speculative features, options and scaffolding.\n2. **Already in this codebase?** Reuse its code, types and patterns.\n3. **Standard library or platform feature?** Use it when it fits.\n4. **Installed dependency?** Reuse it; add one only for a concrete benefit.\n5. **Direct expression?** Use a clear one-liner.\n6. **Otherwise:** write the smallest cohesive implementation.\n\n## Design judgement\n\n- Remove a wrapper that adds no useful contract; keep a helper that adds\n  clarity or removes real duplication. One implementation is not bloat.\n- Prefer fewer concepts; diff size breaks ties after correctness and clarity.\n- Keep every changed line traceable to the request. Remove what your change\n  made unused; report other dead code. Preserve unrelated work.\n- Choose algorithms by their semantics, failure modes and resource costs;\n  keep the project's optimised stack.\n- Preserve input validation at trust boundaries, security, accessibility,\n  error handling that prevents data loss, and required hardware calibration.\n  In data, money and security paths, reject malformed input or report what\n  was skipped.\n- Mark a deliberate corner cut with a `shortcut:` comment naming the ceiling and\n  upgrade path. No other justification comments or Puritanical markers.\n\n## Verification and reporting\n\nUse the project's test tools and required checks. Add a focused test of the\ncontract for changed failure modes when existing coverage is insufficient; a\nregression test must be one that fails without the fix. Money and security\ncode can need several cases. There is no one-test ceiling. Never weaken, skip or\ndelete a test or check to make it pass; change an assertion only when it tests\nthe wrong contract, and say why. Reuse valid results. A timeout is inconclusive.\n\nClaim only what this session's evidence shows: name each check and its\nresult, and mark the rest not run. Inspect generated artefacts before calling\nthem usable. Lead with the result. Report a small change in a few lines: what\nchanged, the check and any limitation. Give requested reports the detail they\nneed.\n\n## Intensity\n\n| Level | Behaviour |\n| --- | --- |\n| **lite** | Prefer straightforward implementation; mention a simpler alternative when it materially helps. |\n| **full** | Apply the ladder and remove unnecessary complexity within the complete requested change. Default. |\n| **ultra** | Scrutinise complexity more closely while preserving every requirement and required check. |\n";

function getFallbackInstructions(mode) {
  return 'PURITANICAL MODE ACTIVE — level: ' + mode + '\n\n' + filterSkillBodyForMode(FALLBACK_SKILL_BODY, mode);
}

function getPuritanicalInstructions(mode) {
  const configuredMode = normalizePersistedMode(mode) || DEFAULT_MODE;

  if (INDEPENDENT_MODES.has(configuredMode)) {
    return 'PURITANICAL MODE ACTIVE — level: ' + configuredMode + '. Behavior defined by /puritanical-' + configuredMode + ' skill.';
  }

  const effectiveMode = normalizeMode(configuredMode) || DEFAULT_MODE;

  try {
    return 'PURITANICAL MODE ACTIVE — level: ' + effectiveMode + '\n\n' +
      filterSkillBodyForMode(fs.readFileSync(SKILL_PATH, 'utf8'), effectiveMode);
  } catch {
    return getFallbackInstructions(effectiveMode);
  }
}

module.exports = {
  filterSkillBodyForMode,
  getFallbackInstructions,
  getPuritanicalInstructions,
};
