#!/usr/bin/env node
/* global Buffer, process, require, setImmediate -- Node.js runtime */

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('child_process');
const {
  fs, absolutePath, listDir, loadModule, makeDir, readJson, readText,
  repoPath, tempRoot, under, writeText,
} = require('./helpers/sandbox');

const SANDBOX = require.resolve('./helpers/sandbox');
const COMPRESS = repoPath('hooks', 'chisle-compress-output.js');
const PROXY = repoPath('hooks', 'chisle-mcp-proxy.js');

const { extractText, recordSavings } = loadModule(COMPRESS);

function isolatedEnv(t, prefix, extra = {}) {
  const dir = tempRoot(t, prefix);
  return {
    dir,
    env: {
      ...process.env,
      HOME: dir,
      USERPROFILE: dir,
      XDG_CONFIG_HOME: under(dir, 'xdg'),
      APPDATA: under(dir, 'appdata'),
      CLAUDE_CONFIG_DIR: under(dir, 'claude'),
      COPILOT_HOME: under(dir, 'copilot'),
      CHISLE_HOME: under(dir, 'chisle'),
      CHISLE_STATE_DIR: under(dir, 'state'),
      ...extra,
    },
  };
}

function bigOutput(lines = 200) {
  return Array.from(
    { length: lines },
    (_, i) => `line ${i} ${'x'.repeat(100)}`,
  ).join('\n');
}

test('extractText returns null when an object cannot be JSON-stringified', () => {
  const circular = { label: 'circular' };
  circular.self = circular;
  assert.equal(extractText(circular), null);
});

test('recordSavings leaves an existing ledger unchanged when opening it is unsafe', (t) => {
  const dir = tempRoot(t, 'chisle-safe-read-');
  const ledger = under(dir, '.chisle-compress-stats.json');
  const original = JSON.stringify({ savedChars: 12, events: 3 });
  writeText(ledger, original);

  const realOpenSync = fs.openSync;
  let openAttempted = false;
  fs.openSync = function patchedOpenSync(file, ...args) {
    if (absolutePath(String(file)) === absolutePath(ledger)) {
      openAttempted = true;
      const error = new Error('access denied by test');
      error.code = 'EACCES';
      throw error;
    }
    return realOpenSync.call(this, file, ...args);
  };

  try {
    assert.doesNotThrow(() => recordSavings(1000, dir));
  } finally {
    fs.openSync = realOpenSync;
  }
  assert.equal(openAttempted, true);
  assert.equal(readText(ledger), original);
});

test('Claude hook writes a shape-preserving replacement, spill file, stats, and successful exit', (t) => {
  const { dir, env } = isolatedEnv(t, 'chisle-claude-main-', {
    CHISLE_DEFAULT_MODE: 'on',
    CHISLE_COMPRESS: '',
    CHISLE_COMPRESS_DEDUP: '0',
    CHISLE_COMPRESS_FILTERS: '0',
    CHISLE_COMPRESS_SCRUB: '0',
    CHISLE_COMPRESS_SPILL: '',
    CHISLE_COMPRESS_MAX_CHARS: '1000',
    CHISLE_COMPRESS_HEAD_LINES: '3',
    CHISLE_COMPRESS_TAIL_LINES: '2',
    CHISLE_COMPRESS_TOOLS: '',
  });

  const claudeDir = env.CLAUDE_CONFIG_DIR;
  makeDir(claudeDir, { recursive: true });
  writeText(under(claudeDir, '.chisle-active'), 'on\n');

  const text = bigOutput();
  const payload = {
    session_id: 'coverage-session',
    tool_use_id: 'coverage-call',
    tool_name: 'Bash',
    tool_input: { command: 'coverage-command' },
    tool_response: {
      stdout: text,
      stderr: '',
      interrupted: false,
    },
  };

  const run = spawnSync(process.execPath, [COMPRESS], {
    input: JSON.stringify(payload),
    encoding: 'utf8',
    timeout: 20000,
    env,
  });

  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stderr, '');
  assert.notEqual(run.stdout, '');

  const answer = JSON.parse(run.stdout);
  assert.equal(answer.hookSpecificOutput.hookEventName, 'PostToolUse');

  const rebuilt = answer.hookSpecificOutput.updatedToolOutput;
  assert.equal(typeof rebuilt, 'object');
  assert.equal(rebuilt.interrupted, false);
  assert.equal(rebuilt.stderr, '');
  assert.ok(rebuilt.stdout.length < text.length);
  assert.match(rebuilt.stdout, /\[chisle: elided/);
  assert.match(rebuilt.stdout, /^line 0 /);
  assert.match(rebuilt.stdout, /line 199 /);

  const statsPath = under(claudeDir, '.chisle-compress-stats.json');
  const stats = readJson(statsPath);
  assert.deepEqual(stats, {
    savedChars: text.length - rebuilt.stdout.length,
    events: 1,
  });

  const spillDir = under(claudeDir, 'chisle-spill');
  const spillFiles = listDir(spillDir).filter(name => name.endsWith('.txt'));
  assert.equal(spillFiles.length, 1);
  assert.equal(readText(under(spillDir, spillFiles[0])), text);

  assert.ok(absolutePath(statsPath).startsWith(absolutePath(dir)));
});

test('MCP proxy prints usage and exits with status 2 when no command is supplied', (t) => {
  const { env } = isolatedEnv(t, 'chisle-proxy-usage-');
  const run = spawnSync(process.execPath, [PROXY], {
    input: '',
    encoding: 'utf8',
    timeout: 10000,
    env,
  });

  assert.equal(run.status, 2);
  assert.equal(run.stdout, '');
  assert.equal(run.stderr, 'usage: chisle-mcp-proxy -- <command> [args...]\n');
});

test('MCP proxy Windows branch quotes special arguments and leaves plain arguments unquoted', (t) => {
  const { env } = isolatedEnv(t, 'chisle-proxy-win-', {
    CHISLE_COMPRESS: '0',
    CHISLE_DEFAULT_MODE: 'on',
  });

  const bootstrap = `
    const { clearModule, writeStdoutSync } = require(${JSON.stringify(SANDBOX)});
    const { EventEmitter } = require('events');
    const Module = require('module');
    const cp = require('child_process');
    const proxy = ${JSON.stringify(PROXY)};
    const descriptor = Object.getOwnPropertyDescriptor(process, 'platform');
    let captured;

    function fakeChild() {
      const child = new EventEmitter();
      child.stdin = new EventEmitter();
      child.stdin.write = () => true;
      child.stdin.end = () => {};
      child.stdout = new EventEmitter();
      child.stdout.pipe = () => {};
      child.stdout.setEncoding = () => {};
      child.stdout.pause = () => {};
      child.stdout.resume = () => {};
      child.kill = () => {};
      return child;
    }

    const child = fakeChild();
    cp.spawn = (file, args, options) => {
      captured = { file, args, options };
      return child;
    };

    Object.defineProperty(process, 'platform', {
      ...descriptor,
      value: 'win32',
    });
    clearModule(proxy);
    process.argv = [
      process.execPath,
      proxy,
      '--',
      'plain',
      '',
      'has space',
      'a"b',
      'tail ' + String.fromCharCode(92),
    ];
    Module._load(proxy, null, true);
    Object.defineProperty(process, 'platform', descriptor);

    setImmediate(() => {
      writeStdoutSync(JSON.stringify(captured));
      child.emit('close', 0, null);
    });
  `;

  const run = spawnSync(process.execPath, ['-e', bootstrap], {
    input: '',
    encoding: 'utf8',
    timeout: 10000,
    env,
  });

  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stderr, '');

  const captured = JSON.parse(run.stdout);
  const expectedCommand = [
    'plain',
    '""',
    '"has space"',
    '"a\\"b"',
    '"tail \\\\"',
  ].join(' ');

  assert.equal(captured.file, expectedCommand);
  assert.deepEqual(captured.args, []);
  assert.deepEqual(captured.options.stdio, ['pipe', 'pipe', 'inherit']);
  assert.equal(captured.options.shell, true);
  assert.equal(captured.options.windowsHide, true);
});

test('MCP proxy pauses server output on backpressure and resumes it after drain', (t) => {
  const { env } = isolatedEnv(t, 'chisle-proxy-backpressure-', {
    CHISLE_COMPRESS: '',
    CHISLE_DEFAULT_MODE: 'on',
  });

  const bootstrap = `
    const { clearModule, writeStdoutSync, writeStderrSync } = require(${JSON.stringify(SANDBOX)});
    const { EventEmitter } = require('events');
    const Module = require('module');
    const cp = require('child_process');
    const proxy = ${JSON.stringify(PROXY)};
    let paused = 0;
    let resumed = 0;
    let blocked = false;

    const child = new EventEmitter();
    child.stdin = new EventEmitter();
    child.stdin.write = () => true;
    child.stdin.end = () => {};
    child.stdout = new EventEmitter();
    child.stdout.setEncoding = () => {};
    child.stdout.pause = () => { paused += 1; };
    child.stdout.resume = () => { resumed += 1; };
    child.stdout.pipe = () => {};
    child.kill = () => {};
    cp.spawn = () => child;

    process.stdout.write = function write(chunk, encoding, callback) {
      if (typeof encoding === 'function') callback = encoding;
      const text = Buffer.isBuffer(chunk) ? chunk.toString('utf8') : String(chunk);
      if (text) writeStdoutSync(text);
      if (callback) queueMicrotask(callback);
      if (!blocked && text.includes('"result"')) {
        blocked = true;
        return false;
      }
      return true;
    };

    clearModule(proxy);
    process.argv = [process.execPath, proxy, '--', 'fake-server'];
    Module._load(proxy, null, true);

    const request = JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method: 'tools/call',
      params: { name: 'run_logs', arguments: {} },
    });
    const response = JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      result: { content: [{ type: 'text', text: 'tiny result' }] },
    });

    process.stdin.emit('data', Buffer.from(request + '\\n'));
    child.stdout.emit('data', response + '\\n');

    setImmediate(() => {
      process.stdout.emit('drain');
      writeStderrSync('REPORT:' + JSON.stringify({ paused, resumed, blocked }));
      child.emit('close', 0, null);
    });
  `;

  const run = spawnSync(process.execPath, ['-e', bootstrap], {
    input: '',
    encoding: 'utf8',
    timeout: 10000,
    env,
  });

  assert.equal(run.status, 0, run.stderr);
  const lines = run.stdout.trim().split('\n');
  assert.equal(lines.length, 1);
  assert.deepEqual(JSON.parse(lines[0]), {
    jsonrpc: '2.0',
    id: 1,
    result: { content: [{ type: 'text', text: 'tiny result' }] },
  });

  assert.match(run.stderr, /^REPORT:/);
  const report = JSON.parse(run.stderr.slice('REPORT:'.length));
  assert.deepEqual(report, { paused: 1, resumed: 1, blocked: true });
});
