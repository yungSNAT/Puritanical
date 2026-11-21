#!/usr/bin/env node
/* global process, require -- Node.js runtime */
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const Module = require('node:module');
const { pathToFileURL } = require('node:url');
const { spawnSync } = require('node:child_process');
const {
  clearModule, isolatedCompressionEnv, loadFresh, readJson, repoPath,
  restoreEnvironment, saveEnvironment, tempRoot, under,
} = require('./helpers/sandbox');

const OPENCODE_PLUGIN = repoPath('.opencode', 'plugins', 'chisle.mjs');
const OPENCODE_SETUP = repoPath('.opencode', 'plugins-v2', 'chisle', 'setup.cjs');

function runScript(script, args, env) {
  return spawnSync(process.execPath, ['-e', script, ...(args || [])], {
    encoding: 'utf8',
    env: env || process.env,
  });
}

test('OpenCode plugin remains a no-op when neither compressor nor config module can be loaded', (t) => {
  const home = tempRoot(t, 'chisle-cov-plugin-null-');

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

  const result = runScript(script, [], isolatedCompressionEnv(home));
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
  const home = tempRoot(t, 'chisle-cov-plugin-config-');
  const stateDir = under(home, 'legacy-state');

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

  const result = runScript(script, [stateDir], isolatedCompressionEnv(home));
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
  const home = tempRoot(t, 'chisle-cov-plugin-root-');

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

  const result = runScript(script, [], isolatedCompressionEnv(home));
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout), {
    output: 'x',
    calls: [{
      chars: 5,
      stateDir: repoPath('.opencode'),
    }],
  });
});

test('OpenCode v2 setup rethrows non-module loading failures', () => {
  const localHookDir = under(path.dirname(OPENCODE_SETUP), 'chisle-hooks') + path.sep;
  const originalLoad = Module._load;

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
      () => loadFresh(OPENCODE_SETUP),
      error => error && error.code === 'EACCES' && /access denied/.test(error.message)
    );
  } finally {
    Module._load = originalLoad;
    clearModule(OPENCODE_SETUP);
  }
});

test('OpenCode v2 setup falls back after a falsy load error and safely handles every content shape', async (t) => {
  const envKeys = [
    'HOME', 'USERPROFILE', 'XDG_CONFIG_HOME', 'CHISLE_STATE_DIR',
    'CHISLE_DEFAULT_MODE', 'CHISLE_COMPRESS_TOOLS', 'CHISLE_COMPRESS',
  ];
  const savedEnv = saveEnvironment(envKeys);
  const originalLoad = Module._load;
  t.after(() => {
    Module._load = originalLoad;
    restoreEnvironment(savedEnv);
    clearModule(OPENCODE_SETUP);
  });

  const home = tempRoot(t, 'chisle-cov-v2-');
  const stateDir = under(home, 'state');
  process.env.HOME = home;
  process.env.USERPROFILE = home;
  process.env.XDG_CONFIG_HOME = under(home, 'xdg');
  process.env.CHISLE_STATE_DIR = stateDir;
  process.env.CHISLE_DEFAULT_MODE = 'on';
  process.env.CHISLE_COMPRESS_TOOLS = 'shell';
  delete process.env.CHISLE_COMPRESS;

  const localHookDir = under(path.dirname(OPENCODE_SETUP), 'chisle-hooks') + path.sep;
  let setup;

  Module._load = function loadWithFalsyLocalFailure(request, parent, isMain) {
    if (typeof request === 'string' && request.startsWith(localHookDir)) throw null;
    return originalLoad.call(this, request, parent, isMain);
  };

  try {
    setup = loadFresh(OPENCODE_SETUP);
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
    const original = Array.from(
      { length: 500 },
      (_, index) => `v2 ${index} ${'x'.repeat(80)}`
    ).join('\n');
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

    const ledger = readJson(under(stateDir, '.chisle-compress-stats.json'));
    assert.deepEqual(ledger, {
      savedChars: original.length - event.result.content[3].text.length,
      events: 1,
    });
  } finally {
    restoreEnvironment(savedEnv);
    clearModule(OPENCODE_SETUP);
  }
});
