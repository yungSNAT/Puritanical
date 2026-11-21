#!/usr/bin/env node
/* global process, require -- Node.js runtime */
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const {
  fs, clearModule, isolatedCompressionEnv, loadFresh, loadModule, makeDir,
  repoPath, restoreEnvironment, saveEnvironment, tempRoot, under, writeJson, writeText,
} = require('./helpers/sandbox');

const REPLAY = repoPath('benchmarks', 'replay-compress.js');
const PI_EXTENSION = repoPath('pi-extension', 'index.js');

test('Pi loadRules returns section-filtered fallback rules when SKILL.md cannot be read', (t) => {
  const home = tempRoot(t, 'chisle-cov-pi-rules-');
  const configRoot = under(home, 'xdg');
  const envKeys = ['HOME', 'USERPROFILE', 'XDG_CONFIG_HOME'];
  const savedEnv = saveEnvironment(envKeys);
  const originalReadFileSync = fs.readFileSync;
  let attemptedSkillRead = false;

  process.env.HOME = home;
  process.env.USERPROFILE = home;
  process.env.XDG_CONFIG_HOME = configRoot;

  makeDir(under(configRoot, 'chisle'), { recursive: true });
  writeJson(under(configRoot, 'chisle', 'config.json'), { sections: { code: false } });

  const skillSuffix = under('skills', 'chisle', 'SKILL.md');
  fs.readFileSync = function failOnlyForSkill(file, ...args) {
    if (String(file).endsWith(skillSuffix)) {
      attemptedSkillRead = true;
      const error = new Error('simulated missing skill');
      error.code = 'ENOENT';
      throw error;
    }
    return originalReadFileSync.call(this, file, ...args);
  };

  try {
    const { loadRules } = loadFresh(PI_EXTENSION);
    const rules = loadRules();

    assert.equal(attemptedSkillRead, true);
    assert.match(rules, /^CHISLE ACTIVE/);
    assert.match(rules, /## Prose/);
    assert.match(rules, /Drop articles, filler, pleasantries/);
    assert.doesNotMatch(rules, /## Code/);
    assert.doesNotMatch(rules, /Code ladder:/);
    assert.match(rules, /## Boundaries/);
  } finally {
    fs.readFileSync = originalReadFileSync;
    restoreEnvironment(savedEnv);
    clearModule(PI_EXTENSION);
  }
});

test('Pi sessionEntries returns an empty list for an unknown reader method', () => {
  const { sessionEntries } = loadModule(PI_EXTENSION);
  let readerCalled = false;
  const ctx = {
    sessionManager: {
      getBranch() {
        readerCalled = true;
        return [{ type: 'unexpected' }];
      },
      buildContextEntries() {
        readerCalled = true;
        return [{ type: 'unexpected' }];
      },
    },
  };

  assert.deepEqual(sessionEntries(ctx, 'unknown-reader'), []);
  assert.equal(readerCalled, false);
});

test('Pi tool-result hook fails open when reading an event throws', async (t) => {
  const home = tempRoot(t, 'chisle-cov-pi-catch-');
  const envKeys = [
    'HOME', 'USERPROFILE', 'XDG_CONFIG_HOME', 'CHISLE_DEFAULT_MODE',
    'CHISLE_COMPRESS', 'CHISLE_COMPRESS_TOOLS',
  ];
  const savedEnv = saveEnvironment(envKeys);

  process.env.HOME = home;
  process.env.USERPROFILE = home;
  process.env.XDG_CONFIG_HOME = under(home, 'xdg');
  process.env.CHISLE_DEFAULT_MODE = 'on';
  delete process.env.CHISLE_COMPRESS;
  delete process.env.CHISLE_COMPRESS_TOOLS;

  try {
    const chisleExtension = loadFresh(PI_EXTENSION);
    const handlers = new Map();
    const pi = {
      on(name, handler) {
        handlers.set(name, handler);
      },
      registerCommand() {},
      appendEntry() {},
      events: {
        on() {
          return () => {};
        },
        emit() {},
      },
    };
    chisleExtension(pi);

    let getterRead = false;
    const event = {};
    Object.defineProperty(event, 'toolName', {
      get() {
        getterRead = true;
        throw new Error('broken event getter');
      },
    });

    const result = await handlers.get('tool_result')(event, { hasUI: false });
    assert.equal(getterRead, true);
    assert.equal(result, undefined);
  } finally {
    restoreEnvironment(savedEnv);
    clearModule(PI_EXTENSION);
  }
});

test('Pi replay resolves default roots and counts every persisted content shape', (t) => {
  const home = tempRoot(t, 'chisle-cov-replay-pi-');

  const agentDir = under(home, 'agent');
  const sessions = under(agentDir, 'sessions');
  makeDir(sessions, { recursive: true });

  const toolText = Array.from(
    { length: 500 },
    (_, index) => `replay ${index} ${'x'.repeat(80)}`
  ).join('\n');
  const entries = [
    { type: 'custom_message', content: 'abc' },
    { type: 'custom_message', content: { ignored: true } },
    {
      type: 'custom_message',
      content: [
        { type: 'text', text: 'four' },
        { type: 'text' },
        null,
        { type: 'image', data: 'ignored' },
      ],
    },
    { type: 'compaction', summary: 'sum' },
    { type: 'branch_summary', summary: 123 },
    { type: 'message', message: { role: 'user', content: 'hello' } },
    { type: 'message', message: { role: 'user', content: { ignored: true } } },
    {
      type: 'message',
      message: {
        role: 'assistant',
        content: [
          { type: 'text', text: 'xy' },
          { type: 'text' },
          { type: 'image', data: 'ignored' },
        ],
      },
    },
    {
      type: 'message',
      message: {
        role: 'toolResult',
        toolName: 'bash',
        content: [{ type: 'text', text: toolText }],
      },
    },
  ];
  writeText(
    under(sessions, 'session.jsonl'),
    entries.map((entry) => JSON.stringify(entry)).join('\n')
  );

  const env = isolatedCompressionEnv(home);
  env.PI_CODING_AGENT_DIR = agentDir;

  const result = spawnSync(process.execPath, [REPLAY, 'pi'], {
    encoding: 'utf8',
    env,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.ok(result.stdout.includes(`transcript root: ${sessions}`));
  assert.match(result.stdout, /tool_results scanned:\s+1 /);
  assert.match(result.stdout, /baseline: persisted Pi output/);

  const savedMatch = result.stdout.match(/^saved:\s+([\d,]+) chars/m);
  assert.ok(savedMatch, result.stdout);
  const saved = Number(savedMatch[1].replaceAll(',', ''));
  assert.ok(saved > 0);

  const ancillaryChars = 3 + 4 + 3 + 3 + 5 + 2;
  const expectedSessionPercent =
    (100 * saved / (toolText.length + ancillaryChars)).toFixed(1);
  assert.ok(
    result.stdout.includes(`${expectedSessionPercent}% of all session content`),
    result.stdout
  );

  const directRoot = under(home, 'direct-sessions');
  makeDir(directRoot);
  const directEnv = isolatedCompressionEnv(home);
  directEnv.PI_CODING_AGENT_SESSION_DIR = directRoot;
  const direct = spawnSync(process.execPath, [REPLAY, 'pi'], {
    encoding: 'utf8',
    env: directEnv,
  });
  assert.equal(direct.status, 0, direct.stderr);
  assert.ok(direct.stdout.includes(`transcript root: ${directRoot}`));

  const homeDefault = under(home, '.pi', 'agent', 'sessions');
  makeDir(homeDefault, { recursive: true });
  const defaultResult = spawnSync(process.execPath, [REPLAY, 'pi'], {
    encoding: 'utf8',
    env: isolatedCompressionEnv(home),
  });
  assert.equal(defaultResult.status, 0, defaultResult.stderr);
  assert.ok(defaultResult.stdout.includes(`transcript root: ${homeDefault}`));
});
