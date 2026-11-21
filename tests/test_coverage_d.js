#!/usr/bin/env node
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Module = require('node:module');
const { pathToFileURL } = require('node:url');
const { spawnSync } = require('node:child_process');

const ROOT = path.join(__dirname, '..');
const OPENCODE_PLUGIN = path.join(ROOT, '.opencode', 'plugins', 'chisle.mjs');
const OPENCODE_SETUP = path.join(ROOT, '.opencode', 'plugins-v2', 'chisle', 'setup.cjs');
const REPLAY = path.join(ROOT, 'benchmarks', 'replay-compress.js');
const PI_EXTENSION = path.join(ROOT, 'pi-extension', 'index.js');

function bigOutput(lines = 500, prefix = 'line') {
  return Array.from(
    { length: lines },
    (_, index) => `${prefix} ${index} ${'x'.repeat(80)}`
  ).join('\n');
}

function isolatedChildEnv(home) {
  const env = {
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: path.join(home, 'xdg'),
    CHISLE_STATE_DIR: path.join(home, 'state'),
    CHISLE_COMPRESS_MAX_CHARS: '8000',
  };
  delete env.CHISLE_COMPRESS;
  delete env.CHISLE_COMPRESS_TOOLS;
  delete env.PI_CODING_AGENT_SESSION_DIR;
  delete env.PI_CODING_AGENT_DIR;
  return env;
}

function runScript(script, args, env) {
  return spawnSync(process.execPath, ['-e', script, ...(args || [])], {
    encoding: 'utf8',
    env: env || process.env,
  });
}

function saveEnvironment(keys) {
  return Object.fromEntries(keys.map(key => [key, process.env[key]]));
}

function restoreEnvironment(saved) {
  for (const [key, value] of Object.entries(saved)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
}

test('OpenCode plugin remains a no-op when neither compressor nor config module can be loaded', (t) => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-plugin-null-'));
  t.after(() => fs.rmSync(home, { recursive: true, force: true }));

  const script = `
    const Module = require('node:module');
    const originalLoad = Module._load;
    const requests = [];
    Module._load = function(request, parent, isMain) {
      if (typeof request === 'string' &&
          (request.endsWith('chisle-compress-output.js') ||
           request.endsWith('chisle-config.js'))) {
        requests.push(request);
        const error = new Error('intentionally missing: ' + request);
        error.code = 'MODULE_NOT_FOUND';
        throw error;
      }
      return originalLoad.call(this, request, parent, isMain);
    };

    import(${JSON.stringify(pathToFileURL(OPENCODE_PLUGIN).href)}).then(async ({ default: plugin }) => {
      const hooks = await plugin({});
      const toolOutput = { output: 'unchanged tool output' };
      const messages = [{
        parts: [{
          type: 'tool',
          tool: 'bash',
          state: { status: 'completed', output: 'unchanged stored output' }
        }]
      }];
      await hooks['tool.execute.after']({ tool: 'bash' }, toolOutput);
      await hooks['experimental.chat.messages.transform']({}, { messages });
      process.stdout.write(JSON.stringify({
        tool: toolOutput.output,
        stored: messages[0].parts[0].state.output,
        requests
      }));
    }).catch(error => {
      console.error(error);
      process.exitCode = 1;
    });
  `;

  const result = runScript(script, [], isolatedChildEnv(home));
  assert.equal(result.status, 0, result.stderr);
  const observed = JSON.parse(result.stdout);
  assert.equal(observed.tool, 'unchanged tool output');
  assert.equal(observed.stored, 'unchanged stored output');
  assert.deepEqual(observed.requests, [
    './chisle-hooks/chisle-compress-output.js',
    '../../hooks/chisle-compress-output.js',
    './chisle-hooks/chisle-config.js',
    '../../hooks/chisle-config.js',
  ]);
});

test('OpenCode plugin uses fallback mode and legacy state-directory accessors', (t) => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-plugin-config-'));
  const stateDir = path.join(home, 'legacy-state');
  t.after(() => fs.rmSync(home, { recursive: true, force: true }));

  const script = `
    const Module = require('node:module');
    const calls = { compress: [], savings: [], stateLookups: 0 };
    const core = {
      limitsFor: () => ({ maxChars: 8000 }),
      compressForOpencode(tool, text, options) {
        calls.compress.push({ tool, text, options });
        return 'cut';
      },
      recordSavings(chars, stateDir) {
        calls.savings.push({ chars, stateDir });
      }
    };
    const config = {
      getOpencodeDir() {
        calls.stateLookups++;
        return process.argv[1];
      }
    };
    const originalLoad = Module._load;
    Module._load = function(request, parent, isMain) {
      if (typeof request === 'string' && request.endsWith('chisle-compress-output.js')) {
        return core;
      }
      if (typeof request === 'string' && request.endsWith('chisle-config.js')) {
        return config;
      }
      return originalLoad.call(this, request, parent, isMain);
    };

    import(${JSON.stringify(pathToFileURL(OPENCODE_PLUGIN).href)}).then(async ({ default: plugin }) => {
      const hooks = await plugin({});
      const output = { output: 'abcdefghij' };
      await hooks['tool.execute.after']({
        tool: 'bash',
        sessionID: 'session-1',
        callID: 'call-1',
        args: { command: 'printf hello' }
      }, output);
      process.stdout.write(JSON.stringify({ output: output.output, calls }));
    }).catch(error => {
      console.error(error);
      process.exitCode = 1;
    });
  `;

  const result = runScript(script, [stateDir], isolatedChildEnv(home));
  assert.equal(result.status, 0, result.stderr);

  const observed = JSON.parse(result.stdout);
  assert.equal(observed.output, 'cut');
  assert.equal(observed.calls.stateLookups, 1);
  assert.deepEqual(observed.calls.compress, [{
    tool: 'bash',
    text: 'abcdefghij',
    options: {
      mode: 'on',
      sessionId: 'session-1',
      callId: 'call-1',
      command: 'printf hello',
    },
  }]);
  assert.deepEqual(observed.calls.savings, [{
    chars: 7,
    stateDir,
  }]);
});

test('OpenCode plugin falls back to its config root when config has no state accessor', (t) => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-plugin-root-'));
  t.after(() => fs.rmSync(home, { recursive: true, force: true }));

  const script = `
    const Module = require('node:module');
    const calls = [];
    const core = {
      limitsFor: () => ({ maxChars: 8000 }),
      compressForOpencode: () => 'x',
      recordSavings(chars, stateDir) {
        calls.push({ chars, stateDir });
      }
    };
    const originalLoad = Module._load;
    Module._load = function(request, parent, isMain) {
      if (typeof request === 'string' && request.endsWith('chisle-compress-output.js')) {
        return core;
      }
      if (typeof request === 'string' && request.endsWith('chisle-config.js')) {
        return {};
      }
      return originalLoad.call(this, request, parent, isMain);
    };

    import(${JSON.stringify(pathToFileURL(OPENCODE_PLUGIN).href)}).then(async ({ default: plugin }) => {
      const hooks = await plugin({});
      const output = { output: 'longer' };
      await hooks['tool.execute.after']({ tool: 'bash' }, output);
      process.stdout.write(JSON.stringify({ output: output.output, calls }));
    }).catch(error => {
      console.error(error);
      process.exitCode = 1;
    });
  `;

  const result = runScript(script, [], isolatedChildEnv(home));
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout), {
    output: 'x',
    calls: [{
      chars: 5,
      stateDir: path.join(ROOT, '.opencode'),
    }],
  });
});

test('OpenCode v2 setup rethrows non-module loading failures', () => {
  const localHookDir = path.join(path.dirname(OPENCODE_SETUP), 'chisle-hooks') + path.sep;
  const originalLoad = Module._load;

  delete require.cache[require.resolve(OPENCODE_SETUP)];
  Module._load = function loadWithDeniedHooks(request, parent, isMain) {
    if (typeof request === 'string' && request.startsWith(localHookDir)) {
      const error = new Error('local hook access denied');
      error.code = 'EACCES';
      throw error;
    }
    return originalLoad.call(this, request, parent, isMain);
  };

  try {
    assert.throws(
      () => require(OPENCODE_SETUP),
      error => error && error.code === 'EACCES' && /access denied/.test(error.message)
    );
  } finally {
    Module._load = originalLoad;
    delete require.cache[require.resolve(OPENCODE_SETUP)];
  }
});

test('OpenCode v2 setup falls back after a falsy load error and safely handles every content shape', async (t) => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-v2-'));
  const stateDir = path.join(home, 'state');
  const envKeys = [
    'HOME', 'USERPROFILE', 'XDG_CONFIG_HOME', 'CHISLE_STATE_DIR',
    'CHISLE_DEFAULT_MODE', 'CHISLE_COMPRESS_TOOLS', 'CHISLE_COMPRESS',
  ];
  const savedEnv = saveEnvironment(envKeys);
  const originalLoad = Module._load;
  t.after(() => {
    Module._load = originalLoad;
    restoreEnvironment(savedEnv);
    delete require.cache[require.resolve(OPENCODE_SETUP)];
  });
  t.after(() => fs.rmSync(home, { recursive: true, force: true }));

  process.env.HOME = home;
  process.env.USERPROFILE = home;
  process.env.XDG_CONFIG_HOME = path.join(home, 'xdg');
  process.env.CHISLE_STATE_DIR = stateDir;
  process.env.CHISLE_DEFAULT_MODE = 'on';
  process.env.CHISLE_COMPRESS_TOOLS = 'shell';
  delete process.env.CHISLE_COMPRESS;

  const localHookDir = path.join(path.dirname(OPENCODE_SETUP), 'chisle-hooks') + path.sep;
  let setup;

  delete require.cache[require.resolve(OPENCODE_SETUP)];
  Module._load = function loadWithFalsyLocalFailure(request, parent, isMain) {
    if (typeof request === 'string' && request.startsWith(localHookDir)) throw null;
    return originalLoad.call(this, request, parent, isMain);
  };

  try {
    setup = require(OPENCODE_SETUP);
  } finally {
    Module._load = originalLoad;
  }

  try {
    let after;
    await setup({
      tool: {
        hook: async (name, callback) => {
          assert.equal(name, 'execute.after');
          after = callback;
        },
      },
    });
    assert.equal(typeof after, 'function');

    await assert.doesNotReject(after(null));

    const running = {
      status: 'running',
      result: { content: 'unchanged while running' },
    };
    await after(running);
    assert.deepEqual(running, {
      status: 'running',
      result: { content: 'unchanged while running' },
    });

    const noResult = { status: 'completed' };
    await after(noResult);
    assert.deepEqual(noResult, { status: 'completed' });

    const unsupportedContent = { opaque: true };
    const unsupportedResult = {
      content: unsupportedContent,
      metadata: { preserved: true },
    };
    const unsupportedEvent = {
      tool: 'shell',
      status: 'completed',
      result: unsupportedResult,
    };
    await after(unsupportedEvent);
    assert.equal(unsupportedEvent.result, unsupportedResult);
    assert.equal(unsupportedEvent.result.content, unsupportedContent);

    const image = { type: 'image', data: 'abc' };
    const original = bigOutput(500, 'v2');
    const event = {
      tool: 'shell',
      status: 'completed',
      sessionID: 'session-v2',
      id: 'call-v2',
      args: { command: 'printf command-branch' },
      result: {
        content: [
          null,
          image,
          { type: 'text', text: 123 },
          { type: 'text', text: original, extra: 'preserved' },
        ],
        metadata: { exit: 0 },
      },
    };

    const previousResult = event.result;
    await after(event);

    assert.notEqual(event.result, previousResult);
    assert.deepEqual(event.result.metadata, { exit: 0 });
    assert.equal(event.result.content[0], null);
    assert.equal(event.result.content[1], image);
    assert.deepEqual(event.result.content[2], { type: 'text', text: 123 });
    assert.equal(event.result.content[3].extra, 'preserved');
    assert.ok(event.result.content[3].text.length < original.length);
    assert.match(event.result.content[3].text, /\[chisle: elided/);

    const ledger = JSON.parse(fs.readFileSync(
      path.join(stateDir, '.chisle-compress-stats.json'),
      'utf8'
    ));
    assert.deepEqual(ledger, {
      savedChars: original.length - event.result.content[3].text.length,
      events: 1,
    });
  } finally {
    restoreEnvironment(savedEnv);
    delete require.cache[require.resolve(OPENCODE_SETUP)];
  }
});

test('Pi loadRules returns section-filtered fallback rules when SKILL.md cannot be read', (t) => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-pi-rules-'));
  const configRoot = path.join(home, 'xdg');
  const envKeys = ['HOME', 'USERPROFILE', 'XDG_CONFIG_HOME'];
  const savedEnv = saveEnvironment(envKeys);
  const originalReadFileSync = fs.readFileSync;
  let attemptedSkillRead = false;

  t.after(() => fs.rmSync(home, { recursive: true, force: true }));
  process.env.HOME = home;
  process.env.USERPROFILE = home;
  process.env.XDG_CONFIG_HOME = configRoot;

  fs.mkdirSync(path.join(configRoot, 'chisle'), { recursive: true });
  fs.writeFileSync(
    path.join(configRoot, 'chisle', 'config.json'),
    JSON.stringify({ sections: { code: false } })
  );

  const skillSuffix = path.join('skills', 'chisle', 'SKILL.md');
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
    delete require.cache[require.resolve(PI_EXTENSION)];
    const { loadRules } = require(PI_EXTENSION);
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
    delete require.cache[require.resolve(PI_EXTENSION)];
  }
});

test('Pi sessionEntries returns an empty list for an unknown reader method', () => {
  const { sessionEntries } = require(PI_EXTENSION);
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
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-pi-catch-'));
  const envKeys = [
    'HOME', 'USERPROFILE', 'XDG_CONFIG_HOME', 'CHISLE_DEFAULT_MODE',
    'CHISLE_COMPRESS', 'CHISLE_COMPRESS_TOOLS',
  ];
  const savedEnv = saveEnvironment(envKeys);
  t.after(() => fs.rmSync(home, { recursive: true, force: true }));

  process.env.HOME = home;
  process.env.USERPROFILE = home;
  process.env.XDG_CONFIG_HOME = path.join(home, 'xdg');
  process.env.CHISLE_DEFAULT_MODE = 'on';
  delete process.env.CHISLE_COMPRESS;
  delete process.env.CHISLE_COMPRESS_TOOLS;

  try {
    delete require.cache[require.resolve(PI_EXTENSION)];
    const chisleExtension = require(PI_EXTENSION);
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
    delete require.cache[require.resolve(PI_EXTENSION)];
  }
});

test('Pi replay resolves default roots and counts every persisted content shape', (t) => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-cov-replay-pi-'));
  t.after(() => fs.rmSync(home, { recursive: true, force: true }));

  const agentDir = path.join(home, 'agent');
  const sessions = path.join(agentDir, 'sessions');
  fs.mkdirSync(sessions, { recursive: true });

  const toolText = bigOutput(500, 'replay');
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
  fs.writeFileSync(
    path.join(sessions, 'session.jsonl'),
    entries.map((entry) => JSON.stringify(entry)).join('\n')
  );

  const env = isolatedChildEnv(home);
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

  const directRoot = path.join(home, 'direct-sessions');
  fs.mkdirSync(directRoot);
  const directEnv = isolatedChildEnv(home);
  directEnv.PI_CODING_AGENT_SESSION_DIR = directRoot;
  const direct = spawnSync(process.execPath, [REPLAY, 'pi'], {
    encoding: 'utf8',
    env: directEnv,
  });
  assert.equal(direct.status, 0, direct.stderr);
  assert.ok(direct.stdout.includes(`transcript root: ${directRoot}`));

  const homeDefault = path.join(home, '.pi', 'agent', 'sessions');
  fs.mkdirSync(homeDefault, { recursive: true });
  const defaultResult = spawnSync(process.execPath, [REPLAY, 'pi'], {
    encoding: 'utf8',
    env: isolatedChildEnv(home),
  });
  assert.equal(defaultResult.status, 0, defaultResult.stderr);
  assert.ok(defaultResult.stdout.includes(`transcript root: ${homeDefault}`));
});
