/* global process, require -- Node.js runtime */
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const TOOLS = require('../bin/lib/tools');
const isolateWorkingDirectory = require('./fixtures/working-directory');

isolateWorkingDirectory();

// A fake Claude config dir: one transcript with a prompt snapshot and some calls.
function fixture(settings) {
  fs.mkdirSync('projects/p', { recursive: true });
  const schema = (name, n) => ({ name, description: 'x'.repeat(n), schema: {} });
  const lines = [
    { type: 'attachment', attachment: { type: 'prompt_snapshot', tools: [schema('Bash', 9000), schema('Artifact', 50000), schema('Workflow', 9000), schema('Tiny', 100), schema('mcp__srv__big', 9000), schema('constructor', 9000)] } },
    { type: 'assistant', message: { id: 'm1', usage: { input_tokens: 1 }, content: [{ type: 'tool_use', name: 'Bash', input: {} }] } },
    { type: 'assistant', message: { id: 'm2', usage: { input_tokens: 1 }, content: [{ type: 'tool_use', name: 'Artifact', input: {} }, { type: 'tool_use', name: 'constructor', input: {} }] } },
  ];
  fs.writeFileSync('projects/p/s.jsonl', lines.map((l) => JSON.stringify(l)).join('\n') + '\n');
  if (settings) fs.writeFileSync('settings.json', JSON.stringify(settings));
  return process.cwd();
}
const settings = () => JSON.parse(fs.readFileSync('settings.json', 'utf8'));
const deny = () => settings().permissions?.deny;

test('tools plan: only heavy, unused, non-core, non-MCP tools are trim candidates', () => {
  const dir = fixture();
  const { rows, requests } = TOOLS.plan(dir, 30);
  const byName = Object.fromEntries(rows.map((r) => [r.name, r]));
  assert.equal(requests, 2);
  assert.equal(byName.Workflow.trim, true);
  assert.equal(byName.Artifact.trim, false);   // used once
  assert.equal(byName.Bash.trim, false);       // core
  assert.equal(byName.constructor.calls, 1);   // ordinary name, even when present on Object.prototype
  assert.equal(byName.Tiny, undefined);        // below size floor
  assert.equal(byName.mcp__srv__big, undefined);
});

test('tools trim then restore leaves the user\'s own deny rules intact', () => {
  const dir = fixture({ permissions: { deny: ['Bash(rm -rf:*)'], allow: ['Read'] }, model: 'opus' });
  const out = [];
  TOOLS.main(['trim', '--also', 'Artifact'], dir, (s) => out.push(s));
  assert.deepEqual(deny(), ['Bash(rm -rf:*)', 'Artifact', 'Workflow']);
  TOOLS.main(['trim'], dir, () => {});         // idempotent
  assert.deepEqual(deny(), ['Bash(rm -rf:*)', 'Artifact', 'Workflow']);
  TOOLS.main(['restore'], dir, () => {});
  const s = settings();
  assert.deepEqual(s.permissions, { deny: ['Bash(rm -rf:*)'], allow: ['Read'] });
  assert.equal(s.model, 'opus');
});

test('tools refuses to deny core tools and changes nothing on --dry-run', () => {
  const dir = fixture({});
  assert.throws(() => TOOLS.main(['trim', '--also', 'Bash'], dir, () => {}), /core/);
  TOOLS.main(['trim', '--dry-run'], dir, () => {});
  assert.equal(deny(), undefined);
});

// ── skills ───────────────────────────────────────────────────────────────────

function skillFixture(settings) {
  fs.mkdirSync('projects/p', { recursive: true });
  const listing = ['- pptx: ' + 'd'.repeat(600), '- plug:used-skill: ' + 'd'.repeat(600), '- mine: ' + 'd'.repeat(600), '- tiny: short'].join('\n');
  const lines = [
    { type: 'attachment', attachment: { type: 'skill_listing', content: listing } },
    { type: 'assistant', message: { id: 'm1', usage: { input_tokens: 1 }, content: [{ type: 'tool_use', name: 'Skill', input: { skill: 'plug:used-skill' } }] } },
  ];
  fs.writeFileSync('projects/p/s.jsonl', lines.map((l) => JSON.stringify(l)).join('\n') + '\n');
  if (settings) fs.writeFileSync('settings.json', JSON.stringify(settings));
  return process.cwd();
}

test('tools trim lists unused skills name-only and leaves used and user-set ones alone', () => {
  const dir = skillFixture({ skillOverrides: { mine: 'off' } });
  const { skills } = TOOLS.plan(dir, 30);
  const byName = Object.fromEntries(skills.map((s) => [s.name, s]));
  assert.equal(byName['plug:used-skill'].trim, false);   // ':' inside a name parses
  assert.equal(byName.pptx.trim, true);
  assert.equal(byName.tiny, undefined);                  // too short to matter
  TOOLS.main(['trim'], dir, () => {});
  const s = settings();
  assert.deepEqual(s.skillOverrides, { mine: 'off', pptx: 'name-only' });
  TOOLS.main(['restore'], dir, () => {});
  assert.deepEqual(settings().skillOverrides, { mine: 'off' });
});

test('tools restore drops skillOverrides when puritanical set every entry', () => {
  const dir = skillFixture({});
  TOOLS.main(['trim'], dir, () => {});
  assert.deepEqual(settings().skillOverrides, { mine: 'name-only', pptx: 'name-only' });
  TOOLS.main(['restore'], dir, () => {});
  assert.equal(Object.hasOwn(settings(), 'skillOverrides'), false);
});
