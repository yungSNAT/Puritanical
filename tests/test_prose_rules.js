#!/usr/bin/env node
/* global require -- Node.js runtime */
// Regression guard: Puritanical's prose rules follow Laconic (plain, complete,
// answer-first sentences). No shipped rule file, fallback ruleset, per-turn
// reminder, manifest, command, skill or README art source may go back to the
// old terse persona.
// Run: node --test tests/test_prose_rules.js

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readText, repoPath } = require('./helpers/sandbox');

// Each entry is a fixed path under the repository, split into segments.
const RULE_FILES = [
  ['skills', 'puritanical', 'SKILL.md'],
  ['AGENTS.md'],
  ['GEMINI.md'],
  ['rules', 'puritanical-activate.md'],
  ['.cursor', 'rules', 'puritanical.mdc'],
  ['.windsurf', 'rules', 'puritanical.md'],
  ['.clinerules', 'puritanical.md'],
  ['.kiro', 'steering', 'puritanical.md'],
  ['.github', 'copilot-instructions.md'],
  ['.agents', 'rules', 'puritanical.md'],
];
const OTHER_SOURCES = [
  ['hooks', 'puritanical-activate.js'],
  ['hooks', 'puritanical-mode-tracker.js'],
  ['pi-extension', 'index.js'],
  ['.claude-plugin', 'plugin.json'],
  ['.claude-plugin', 'marketplace.json'],
  ['.github', 'plugin', 'plugin.json'],
  ['.github', 'plugin', 'marketplace.json'],
  ['.codex-plugin', 'plugin.json'],
  ['gemini-extension.json'],
  ['package.json'],
  ['commands', 'puritanical.toml'],
  ['commands', 'puritanical-audit.toml'],
  ['commands', 'puritanical-help.toml'],
  ['commands', 'puritanical-review.toml'],
  ['skills', 'puritanical-help', 'SKILL.md'],
  ['skills', 'puritanical-audit', 'SKILL.md'],
  ['tools', 'src', 'readme_art', 'banner.rs'],
  ['tools', 'src', 'readme_art', 'coffee.rs'],
  ['tools', 'src', 'readme_art', 'ladder.rs'],
  ['tools', 'src', 'readme_art', 'mod.rs'],
  ['tools', 'src', 'readme_art', 'race.rs'],
  ['tools', 'src', 'readme_art', 'strings.rs'],
];
const OLD_PROSE = /drop articles|fragments|pleasantries|zero[- ]fluff|signal per token|causality as arrows|structure is tokens|terse ≠|bills by the syllable/i;
// pi-extension/index.js still recognises an AGENTS.md copied before this change.
const LEGACY_HEADING = "'## Prose: zero fluff'";

const read = parts => readText(repoPath(...parts)).split(LEGACY_HEADING).join('');

for (const parts of [...RULE_FILES, ...OTHER_SOURCES]) {
  test(`${parts.join('/')} does not ask for the old terse prose`, () => {
    assert.doesNotMatch(read(parts), OLD_PROSE);
  });
}

// The fallback rulesets and the per-turn reminder must say the same thing as the
// rule files, so a later sync that misses one of them fails here.
const FALLBACK_SOURCES = [
  ['hooks', 'puritanical-activate.js'],
  ['pi-extension', 'index.js'],
  ['hooks', 'puritanical-mode-tracker.js'],
];

for (const parts of [...RULE_FILES, ...FALLBACK_SOURCES]) {
  test(`${parts.join('/')} carries the Laconic prose rules`, () => {
    const text = read(parts);
    assert.match(text, /answer first|Lead with the\s+answer/i);
    assert.match(text, /preamble/i);
  });
}

test('the skill ships its prose rules under the Laconic heading', () => {
  assert.match(read(['skills', 'puritanical', 'SKILL.md']), /^## Prose: Laconic$/m);
});
