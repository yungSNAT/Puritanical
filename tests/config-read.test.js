#!/usr/bin/env node
// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: FSL-1.1-ALv2

// A config file that cannot supply settings must read as no settings, whatever is in it.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const { getConfigPath, getDefaultMode, getHideStatus, getQuietStartup, readJsonFile } = require('../hooks/puritanical-config');
const { getPuritanicalInstructions } = require('../hooks/puritanical-instructions');

const SETTINGS = ['PURITANICAL_DEFAULT_MODE', 'PURITANICAL_HIDE_STATUS', 'PURITANICAL_QUIET_STARTUP'];

function withConfig(t, content) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-read-'));
  const saved = process.env;
  t.after(() => {
    process.env = saved;
    fs.rmSync(dir, { recursive: true, force: true });
  });
  process.env = { ...saved };
  for (const name of SETTINGS) delete process.env[name];
  process.env.XDG_CONFIG_HOME = dir;
  fs.mkdirSync(path.join(dir, 'puritanical'));
  const file = path.join(dir, 'puritanical', 'config.json');
  if (content !== null) fs.writeFileSync(file, content);
  return file;
}

for (const [name, content] of [
  ['missing', null], ['malformed', '{ "hideStatus": true,'], ['null', 'null'], ['false', 'false'], ['a number', '5'],
]) {
  test(`a ${name} config file gives the defaults`, (t) => {
    withConfig(t, content);
    assert.equal(getDefaultMode(), 'full');
    assert.equal(getHideStatus(), false);
    assert.equal(getQuietStartup(), false);
  });
}

test('settings are read through a UTF-8 BOM', (t) => {
  withConfig(t, '\uFEFF{ "defaultMode": "lite", "hideStatus": true, "quietStartup": true }');
  assert.equal(getDefaultMode(), 'lite');
  assert.equal(getHideStatus(), true);
  assert.equal(getQuietStartup(), true);
});

test('a malformed settings.json leaves the statusline alone', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-settings-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  fs.writeFileSync(path.join(dir, 'settings.json'), '{ "statusLine": ');
  const env = { ...process.env, CLAUDE_CONFIG_DIR: dir, CLAUDE_PROJECT_DIR: '', XDG_CONFIG_HOME: dir };
  for (const name of ['PLUGIN_DATA', 'COPILOT_PLUGIN_DATA', 'CURSOR_VERSION', 'QODER_PROJECT_DIR', 'CODEBUDDY_PLUGIN_ROOT', 'PURITANICAL_DEFAULT_MODE']) delete env[name];
  const hook = path.join(__dirname, '..', 'hooks', 'puritanical-activate.js');
  const result = spawnSync(process.execPath, [hook], { env, input: '', encoding: 'utf8', timeout: 30000 });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /PURITANICAL MODE ACTIVE/);
  assert.doesNotMatch(result.stdout, /STATUSLINE/);
  for (const name of ['puritanical-statusline.ps1', 'puritanical-statusline.sh', '.puritanical-statusline-nudged']) {
    assert.equal(fs.existsSync(path.join(dir, name)), false, name);
  }
});

test('readJsonFile keeps a missing file and malformed JSON apart for its callers', (t) => {
  const file = withConfig(t, '{ broken');
  assert.throws(() => readJsonFile(file), SyntaxError);
  fs.rmSync(file);
  assert.throws(() => readJsonFile(file), { code: 'ENOENT' });
});

test('Windows config uses APPDATA or falls back to the roaming profile', (t) => {
  const descriptor = Object.getOwnPropertyDescriptor(process, 'platform');
  const saved = process.env;
  const home = path.join(os.tmpdir(), 'fixture-profile');
  t.after(() => {
    Object.defineProperty(process, 'platform', descriptor);
    process.env = saved;
  });
  process.env = { ...saved };
  Object.defineProperty(process, 'platform', { ...descriptor, value: 'win32' });
  t.mock.method(os, 'homedir', () => home);
  delete process.env.XDG_CONFIG_HOME;
  process.env.APPDATA = path.join(home, 'custom-appdata');
  assert.equal(getConfigPath(), path.join(home, 'custom-appdata', 'puritanical', 'config.json'));
  delete process.env.APPDATA;
  assert.equal(getConfigPath(), path.join(home, 'AppData', 'Roaming', 'puritanical', 'config.json'));
});

test('instructions fall back to the embedded rules when the skill is unreadable', (t) => {
  const source = path.join(__dirname, '..', 'skills', 'puritanical', 'SKILL.md');
  const read = fs.readFileSync;
  t.mock.method(fs, 'readFileSync', (file, ...options) => {
    if (file === source) throw Object.assign(new Error('skill missing'), { code: 'ENOENT' });
    return read(file, ...options);
  });
  const fallback = getPuritanicalInstructions('lite');
  assert.match(fallback, /level: lite/);
  assert.match(fallback, /Complete the requested work/);
  assert.match(fallback, /\*\*lite\*\*/);
  assert.doesNotMatch(fallback, /\*\*ultra\*\*/);
});

test('review mode points to its independent skill without injecting the full rules', () => {
  const instructions = getPuritanicalInstructions('review');
  assert.match(instructions, /level: review/);
  assert.match(instructions, /\/puritanical-review skill/);
  assert.doesNotMatch(instructions, /Complete the requested work/);
});
