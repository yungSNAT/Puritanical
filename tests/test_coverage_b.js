#!/usr/bin/env node
/* global __dirname, process, require -- Node.js runtime */

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');

const CONFIG = require('../hooks/chisle-config');
const { filterCommand } = require('../hooks/chisle-command-filters');

const ACTIVATE = path.join(__dirname, '..', 'hooks', 'chisle-activate.js');
const TRACKER = path.join(__dirname, '..', 'hooks', 'chisle-mode-tracker.js');

function tempDir(t, prefix) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function setEnv(t, values) {
  const previous = new Map();
  for (const [name, value] of Object.entries(values)) {
    previous.set(name, Object.prototype.hasOwnProperty.call(process.env, name)
      ? process.env[name]
      : undefined);
    if (value === undefined) delete process.env[name];
    else process.env[name] = String(value);
  }

  t.after(() => {
    for (const [name, value] of previous) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  });
}

function childEnv(root, claudeDir, xdgDir, extra = {}) {
  const home = path.join(root, 'home');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(claudeDir, { recursive: true });
  fs.mkdirSync(xdgDir, { recursive: true });
  return {
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    CLAUDE_CONFIG_DIR: claudeDir,
    XDG_CONFIG_HOME: xdgDir,
    CHISLE_DEFAULT_MODE: 'on',
    CHISLE_UPDATE_CHECK: '0',
    ...extra,
  };
}

test('activate emits the fallback ruleset and writes the active flag when SKILL.md is unavailable', (t) => {
  const root = tempDir(t, 'chisle-activate-fallback-');
  const claudeDir = path.join(root, 'claude');
  const xdgDir = path.join(root, 'xdg');
  const skillPath = path.join(__dirname, '..', 'skills', 'chisle', 'SKILL.md');
  const preload = path.join(root, 'hide-skill.js');

  fs.writeFileSync(preload, [
    "const fs = require('fs');",
    "const path = require('path');",
    'const original = fs.readFileSync;',
    'fs.readFileSync = function (filename, ...args) {',
    '  if (path.resolve(String(filename)) === path.resolve(process.env.CHISLE_TEST_MISSING_SKILL)) {',
    "    const error = new Error('SKILL.md intentionally unavailable');",
    "    error.code = 'ENOENT';",
    '    throw error;',
    '  }',
    '  return original.call(this, filename, ...args);',
    '};',
  ].join('\n'));

  const result = spawnSync(process.execPath, ['--require', preload, ACTIVATE], {
    cwd: root,
    input: JSON.stringify({ source: 'startup' }),
    env: childEnv(root, claudeDir, xdgDir, {
      CHISLE_TEST_MISSING_SKILL: skillPath,
    }),
    encoding: 'utf8',
    timeout: 5000,
  });

  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.signal, null);
  assert.equal(result.stderr, '');
  assert.ok(result.stdout.startsWith('CHISLE ACTIVE\n\n'));
  assert.ok(result.stdout.includes(
    'Chisle: maximum-efficiency dev mode. Zero-fluff prose. YAGNI-first code.'
  ));
  assert.ok(result.stdout.includes('## Persistence'));
  assert.ok(result.stdout.includes('## Prose'));
  assert.ok(result.stdout.includes('Drop articles/filler/pleasantries/hedging.'));
  assert.ok(result.stdout.includes('## Code'));
  assert.ok(result.stdout.includes('Ladder: YAGNI → reuse → stdlib → native'));
  assert.ok(result.stdout.includes('STATUSLINE SETUP NEEDED:'));
  assert.ok(!result.stdout.includes('## Thinking Is Billed Too'));
  assert.equal(
    fs.readFileSync(path.join(claudeDir, '.chisle-active'), 'utf8'),
    'on'
  );
});

test('getConfigDir uses the Windows roaming-profile fallback when APPDATA is absent', (t) => {
  const root = tempDir(t, 'chisle-win-config-');
  const home = path.join(root, 'profile');
  fs.mkdirSync(home, { recursive: true });

  setEnv(t, {
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: undefined,
    APPDATA: undefined,
  });
  t.mock.method(os, 'homedir', () => home);

  const platformDescriptor = Object.getOwnPropertyDescriptor(process, 'platform');
  try {
    Object.defineProperty(process, 'platform', {
      ...platformDescriptor,
      value: 'win32',
    });
    assert.equal(
      CONFIG.getConfigDir(),
      path.join(home, 'AppData', 'Roaming', 'chisle')
    );
  } finally {
    Object.defineProperty(process, 'platform', platformDescriptor);
  }
});

test('getDefaultMode reads and normalizes a valid mode from config.json', (t) => {
  const root = tempDir(t, 'chisle-default-config-');
  const home = path.join(root, 'home');
  const xdgDir = path.join(root, 'xdg');
  const configDir = path.join(xdgDir, 'chisle');

  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(configDir, { recursive: true });
  fs.writeFileSync(
    path.join(configDir, 'config.json'),
    JSON.stringify({ defaultMode: 'OFF' })
  );

  setEnv(t, {
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: xdgDir,
    CHISLE_DEFAULT_MODE: undefined,
  });

  assert.equal(CONFIG.getDefaultMode(), 'off');
});

test('filterSections returns the original content when section inspection throws', () => {
  const content = 'preamble\n\n## Prose\n\nKeep the original text.';
  const sections = {};
  let proseRead = false;
  Object.defineProperty(sections, 'prose', {
    get() {
      proseRead = true;
      throw new Error('unreadable sections');
    },
  });

  assert.equal(CONFIG.filterSections(content, sections), content);
  assert.equal(proseRead, true);
});

test('mode tracker ignores an unreadable transcript while still reinforcing active mode', (t) => {
  const root = tempDir(t, 'chisle-tracker-transcript-');
  const claudeDir = path.join(root, 'claude');
  const xdgDir = path.join(root, 'xdg');
  const flagPath = path.join(claudeDir, '.chisle-active');

  fs.mkdirSync(claudeDir, { recursive: true });
  fs.writeFileSync(flagPath, 'on', { mode: 0o600 });

  const result = spawnSync(process.execPath, [TRACKER], {
    cwd: root,
    input: JSON.stringify({
      prompt: 'continue',
      transcript_path: path.join(root, 'missing-session.jsonl'),
    }),
    env: childEnv(root, claudeDir, xdgDir),
    encoding: 'utf8',
    timeout: 5000,
  });

  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.signal, null);
  assert.equal(result.stderr, '');

  const output = JSON.parse(result.stdout);
  assert.equal(output.systemMessage, undefined);
  assert.equal(
    output.hookSpecificOutput.hookEventName,
    'UserPromptSubmit'
  );
  assert.ok(
    output.hookSpecificOutput.additionalContext.startsWith('CHISLE ACTIVE.')
  );
  assert.equal(fs.readFileSync(flagPath, 'utf8'), 'on');
});

test('mode tracker silently exits without changing state when its input is malformed', (t) => {
  const root = tempDir(t, 'chisle-tracker-malformed-');
  const claudeDir = path.join(root, 'claude');
  const xdgDir = path.join(root, 'xdg');
  const flagPath = path.join(claudeDir, '.chisle-active');
  const markerPath = path.join(root, 'parse-attempted');
  const preload = path.join(root, 'track-json-parse.js');
  const malformed = '{ malformed input';

  fs.mkdirSync(claudeDir, { recursive: true });
  fs.writeFileSync(flagPath, 'on', { mode: 0o600 });
  fs.writeFileSync(preload, [
    "const fs = require('fs');",
    'const originalParse = JSON.parse;',
    'JSON.parse = function trackedParse(value, ...args) {',
    '  if (value === process.env.CHISLE_TEST_MALFORMED_PAYLOAD) {',
    "    fs.writeFileSync(process.env.CHISLE_TEST_PARSE_MARKER, 'attempted');",
    '  }',
    '  return originalParse.call(this, value, ...args);',
    '};',
  ].join('\n'));

  const result = spawnSync(process.execPath, ['--require', preload, TRACKER], {
    cwd: root,
    input: malformed,
    env: childEnv(root, claudeDir, xdgDir, {
      CHISLE_TEST_MALFORMED_PAYLOAD: malformed,
      CHISLE_TEST_PARSE_MARKER: markerPath,
    }),
    encoding: 'utf8',
    timeout: 5000,
  });

  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.signal, null);
  assert.equal(result.stdout, '');
  assert.equal(result.stderr, '');
  assert.equal(fs.existsSync(markerPath), true);
  assert.equal(fs.readFileSync(flagPath, 'utf8'), 'on');
});

test('grep grouping preserves separator context and resets after an unmatched line', () => {
  const input = [
    'src/a.js:1:first',
    'src/a.js:2:second',
    '--',
    'src/a.js-3-context',
    'unmatched marker',
    'src/a.js:4:fourth',
  ].join('\n');

  assert.equal(filterCommand('grep -rn needle src', input), [
    'src/a.js',
    '1:first',
    '2:second',
    '--',
    '3-context',
    'unmatched marker',
    'src/a.js',
    '4:fourth',
  ].join('\n'));
});
