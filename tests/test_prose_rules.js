#!/usr/bin/env node
/* global __dirname, require -- Node.js runtime */
// Regression guard: Puritanical's prose rules follow Laconic (plain, complete,
// answer-first sentences). No shipped rule file, fallback ruleset or per-turn
// reminder may go back to telling the model to drop articles or write fragments.
// Run: node --test tests/test_prose_rules.js

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const read = rel => fs.readFileSync(path.join(ROOT, rel), 'utf8'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

const RULE_FILES = [
  'skills/puritanical/SKILL.md',
  'AGENTS.md',
  'GEMINI.md',
  'rules/puritanical-activate.md',
  '.cursor/rules/puritanical.mdc',
  '.windsurf/rules/puritanical.md',
  '.clinerules/puritanical.md',
  '.kiro/steering/puritanical.md',
  '.github/copilot-instructions.md',
  '.agents/rules/puritanical.md',
];
// Fallback rulesets and the per-turn reminder live in code.
const RULE_SOURCES = [
  'hooks/puritanical-activate.js',
  'hooks/puritanical-mode-tracker.js',
  'pi-extension/index.js',
];
const OLD_PROSE = /drop articles|fragments|pleasantries|default to fragments/i;

for (const rel of [...RULE_FILES, ...RULE_SOURCES]) {
  test(`${rel} does not ask for fragments or dropped articles`, () => {
    assert.doesNotMatch(read(rel), OLD_PROSE);
  });
}

for (const rel of RULE_FILES) {
  test(`${rel} carries the Laconic prose rules`, () => {
    assert.match(read(rel), /answer first|Lead with the\s+answer/i);
    assert.match(read(rel), /preamble/i);
  });
}

test('the skill ships its prose rules under the Laconic heading', () => {
  assert.match(read('skills/puritanical/SKILL.md'), /^## Prose: Laconic$/m);
});
