/* global module, process, require -- Node.js runtime */
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('path');
const { spawnSync } = require('child_process');
const Module = require('module');
const {
  clearModule, exists, loadFresh, listDir, makeDir, readJson, readText,
  repoPath, tempRoot, under, writeJson, writeText,
} = require('./helpers/sandbox');

const CLI = repoPath('bin', 'install.js');
const ENV_PATH = repoPath('bin', 'lib', 'env.js');
const SETTINGS = require('../bin/lib/settings');
const TOOLS = require('../bin/lib/tools');

function tempDir(t, prefix = 'puritanical-coverage-a-') {
  return tempRoot(t, prefix);
}

function isolatedEnv(home, extra = {}) {
  return {
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: under(home, 'xdg'),
    PURITANICAL_HOME: home,
    CLAUDE_CONFIG_DIR: '',
    COPILOT_HOME: '',
    OPENCODE_CONFIG_DIR: '',
    NO_COLOR: '1',
    CI: '1',
    ...extra,
  };
}

function runCLI(args, { cwd, env } = {}) {
  const result = spawnSync(process.execPath, [CLI, ...args, '--no-color'], {
    cwd,
    env,
    input: '',
    encoding: 'utf8',
    timeout: 15000,
  });
  return {
    status: result.status === null ? 1 : result.status,
    out: String(result.stdout || '') + String(result.stderr || ''),
    error: result.error,
  };
}

function writeFakeCommand(binDir, name, posixBody, windowsBody) {
  makeDir(binDir, { recursive: true });
  if (process.platform === 'win32') {
    const file = under(binDir, `${name}.cmd`);
    writeText(file, `@echo off\r\n${windowsBody.replace(/\n/g, '\r\n')}\r\n`);
    return file;
  }
  const file = under(binDir, name);
  writeText(file, `#!/bin/sh\n${posixBody}\n`, { mode: 0o755 });
  return file;
}

function withEnv(changes, fn) {
  const previous = new Map();
  for (const [key, value] of Object.entries(changes)) {
    previous.set(key, Object.prototype.hasOwnProperty.call(process.env, key)
      ? process.env[key] // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
      : undefined);
    if (value === undefined) delete process.env[key]; // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
    else process.env[key] = String(value); // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
  }
  try {
    return fn();
  } finally {
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key]; // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
      else process.env[key] = value; // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
    }
  }
}

// env.js reads process.platform once, at load.
function loadEnvFor(platform) {
  const descriptor = Object.getOwnPropertyDescriptor(process, 'platform');
  Object.defineProperty(process, 'platform', {
    configurable: true,
    enumerable: descriptor.enumerable,
    value: platform,
  });
  try {
    return loadFresh(ENV_PATH);
  } finally {
    Object.defineProperty(process, 'platform', descriptor);
    clearModule(ENV_PATH);
  }
}

function loadInstallerInternals() {
  clearModule(CLI);
  let source = readText(CLI).replace(/^#!/, '//');
  assert.match(source, /\nmain\(\);\s*$/);
  source = source.replace(/\nmain\(\);\s*$/, '\nmodule.exports = { installedFor };\n');
  const mod = new Module(CLI, module);
  mod.filename = CLI;
  mod.paths = Module._nodeModulePaths(path.dirname(CLI));
  mod._compile(source, CLI);
  return mod.exports;
}

test('Windows command quoting quotes empty and spaced arguments while preserving plain arguments', () => {
  const windowsEnv = loadEnvFor('win32');
  assert.equal(windowsEnv.quoteWin(''), '""');
  assert.equal(windowsEnv.quoteWin('two words'), '"two words"');
  assert.equal(windowsEnv.quoteWin('a"b'), '"a\\"b"');
  // Every trailing backslash is doubled, so the closing quote stays a quote.
  assert.equal(windowsEnv.quoteWin('a b\\\\'), '"a b\\\\\\\\"');
  assert.equal(windowsEnv.quoteWin('a\\\\"b'), '"a\\\\\\\\\\"b"');
  assert.equal(windowsEnv.quoteWin('plain'), 'plain');

  const nonWindowsEnv = loadEnvFor('linux');
  assert.equal(nonWindowsEnv.quoteWin('two words'), 'two words');
});

test('capture enforces a bounded timeout and SIGKILL on hung child processes', (t) => {
  const cp = require('child_process');
  const envMod = loadFresh(ENV_PATH);
  t.after(() => clearModule(ENV_PATH));

  const fast = envMod.capture('node', ['-e', "process.stdout.write('ok')"]);
  assert.equal(envMod.ranOk(fast), true);
  assert.equal(fast.stdout, 'ok');

  const warned = [];
  const stderrWrite = t.mock.method(process.stderr, 'write', (chunk) => { warned.push(String(chunk)); return true; });
  const started = Date.now();
  const hung = envMod.capture('node', ['-e', 'setTimeout(() => {}, 5000)'], 150);
  const elapsed = Date.now() - started;
  stderrWrite.mock.restore();
  assert.equal(warned.length, 1);
  assert.match(warned[0], /warning: `node -e setTimeout\(\(\) => \{\}, 5000\)` gave no answer within 1 s; its result was skipped/);
  assert.equal(envMod.ranOk(hung), false);
  assert.equal(hung.status, null);
  assert.equal(hung.error && hung.error.code, 'ETIMEDOUT');
  assert.ok(elapsed < 4000, `capture took ${elapsed}ms instead of timing out`);

  let capturedOpts;
  t.mock.method(cp, 'spawnSync', (_cmd, _args, opts) => {
    capturedOpts = opts;
    return { status: 0, stdout: '', stderr: '' };
  });
  envMod.capture('probe', ['--version']);
  assert.equal(capturedOpts.timeout, envMod.CAPTURE_TIMEOUT_MS);
  assert.equal(capturedOpts.killSignal, 'SIGKILL');
  assert.equal(capturedOpts.encoding, 'utf8');
});

test('default Claude directory uses the isolated home and foreign hooks are not treated as installed', (t) => {
  const root = tempDir(t);
  const home = under(root, 'home');
  const emptyPath = under(root, 'empty-path');
  const cfg = under(home, '.claude');
  const settingsPath = under(cfg, 'settings.json');
  makeDir(cfg, { recursive: true });
  makeDir(emptyPath);
  writeJson(settingsPath, {
    hooks: {
      SessionStart: [{ hooks: [{ type: 'command', command: 'node foreign-startup.js' }] }],
    },
  });

  const installer = loadInstallerInternals();
  const claude = { id: 'claude' };
  withEnv({
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: under(home, 'xdg'),
    PURITANICAL_HOME: undefined,
    CLAUDE_CONFIG_DIR: undefined,
    PATH: emptyPath,
  }, () => {
    assert.equal(loadEnvFor(process.platform).claudeDir({}), cfg);
    assert.equal(installer.installedFor(claude, {}), false);
    writeJson(settingsPath, {
      hooks: {
        SessionStart: [{ hooks: [{ type: 'command', command: 'node puritanical-activate.js' }] }],
      },
    });
    assert.equal(installer.installedFor(claude, {}), true);
  });
});

function fakeClaude(t, logName, unixLines, windowsLines) {
  const root = tempDir(t);
  const home = under(root, 'home');
  const cfg = under(root, 'claude-config');
  const bin = under(root, 'bin');
  const log = under(root, logName);
  makeDir(home);
  makeDir(cfg);
  writeFakeCommand(bin, 'claude', unixLines.join('\n'), windowsLines.join('\n'));
  return { root, home, cfg, bin, log };
}

test('failed Claude plugin installation warns and falls back to working standalone hooks', (t) => {
  const { root, home, cfg, bin, log } = fakeClaude(t, 'claude.log', [
    'echo "$@" >> "$FAKE_LOG"',
    'if [ "$1" = "plugin" ] && [ "$2" = "list" ]; then',
    '  echo none',
    '  exit 0',
    'fi',
    'exit 1',
  ], [
    'echo %*>>"%FAKE_LOG%"',
    'if "%1 %2"=="plugin list" (',
    '  echo none',
    '  exit /b 0',
    ')',
    'exit /b 1',
  ]);
  const env = isolatedEnv(home, {
    PATH: bin + path.delimiter + process.env.PATH,
    FAKE_LOG: log,
  });
  const result = runCLI(['--only', 'claude', '--config-dir', cfg], { cwd: root, env });

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /plugin install failed, falling back to standalone hooks/);
  assert.match(result.out, /installed:\s+claude-hooks/);
  const calls = readText(log);
  assert.match(calls, /plugin marketplace add yungSNAT\/Puritanical/);
  assert.match(calls, /plugin install puritanical@puritanical/);

  const settings = readJson(under(cfg, 'settings.json'));
  assert.ok(settings.hooks.SessionStart);
  assert.ok(settings.hooks.PostToolUse);
  assert.ok(exists(under(cfg, 'puritanical-hooks', 'puritanical-compress-output.js')));
});

test('unparseable Claude settings produce a failed install and preserve the original file', (t) => {
  const root = tempDir(t);
  const home = under(root, 'home');
  const cfg = under(root, 'claude-config');
  const emptyPath = under(root, 'empty-path');
  makeDir(home);
  makeDir(cfg);
  makeDir(emptyPath);
  const settingsPath = under(cfg, 'settings.json');
  const invalid = '{ this is not valid JSON';
  writeText(settingsPath, invalid);

  const result = runCLI(['--only', 'claude', '--config-dir', cfg], {
    cwd: root,
    env: isolatedEnv(home, { PATH: emptyPath }),
  });
  assert.equal(result.status, 1, result.out);
  assert.match(result.out, /settings\.json unparseable/);
  assert.match(result.out, /failed:\s+claude-hooks \(settings\.json unparseable\)/);
  assert.equal(readText(settingsPath), invalid);
});

test('existing non-Puritanical statusline is preserved and reported instead of overwritten', (t) => {
  const root = tempDir(t);
  const home = under(root, 'home');
  const cfg = under(root, 'claude-config');
  const emptyPath = under(root, 'empty-path');
  makeDir(home);
  makeDir(cfg);
  makeDir(emptyPath);
  const settingsPath = under(cfg, 'settings.json');
  writeJson(settingsPath, {
    model: 'opus',
    statusLine: { type: 'command', command: 'my-existing-statusline' },
  });

  const result = runCLI(['--only', 'claude', '--config-dir', cfg], {
    cwd: root,
    env: isolatedEnv(home, { PATH: emptyPath }),
  });
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /existing statusline detected, PURITANICAL badge NOT added/);
  const settings = readJson(settingsPath);
  assert.deepEqual(settings.statusLine, {
    type: 'command',
    command: 'my-existing-statusline',
  });
  assert.ok(settings.hooks.SessionStart);
});

test('Gemini reports an existing extension as skipped and does not reinstall it', (t) => {
  const root = tempDir(t);
  const home = under(root, 'home');
  const bin = under(root, 'bin');
  const log = under(root, 'gemini.log');
  makeDir(home);
  writeFakeCommand(bin, 'gemini', [
    'echo "$@" >> "$FAKE_LOG"',
    'if [ "$1" = "extensions" ] && [ "$2" = "list" ]; then echo puritanical; fi',
    'exit 0',
  ].join('\n'), [
    'echo %*>>"%FAKE_LOG%"',
    'if "%1 %2"=="extensions list" echo puritanical',
    'exit /b 0',
  ].join('\n'));

  const result = runCLI(['--only', 'gemini'], {
    cwd: root,
    env: isolatedEnv(home, {
      PATH: bin + path.delimiter + process.env.PATH,
      FAKE_LOG: log,
    }),
  });
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /puritanical extension already installed/);
  assert.match(result.out, /skipped:\s+gemini/);
  const calls = readText(log);
  assert.match(calls, /extensions list/);
  assert.doesNotMatch(calls, /extensions install/);
});

test('Codex installation separates an unterminated user file from the fenced ruleset', (t) => {
  const root = tempDir(t);
  const home = under(root, 'home');
  const target = under(home, '.codex', 'AGENTS.md');
  makeDir(path.dirname(target), { recursive: true });
  writeText(target, 'user instructions without newline');

  const result = runCLI(['--only', 'codex'], { cwd: root, env: isolatedEnv(home) });
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /installed:/);
  const written = readText(target);
  assert.ok(written.startsWith('user instructions without newline\n\n<!-- puritanical-begin -->\n'));
  assert.match(written, /<!-- puritanical-end -->\n$/);
});

test('fenced installation preserves an existing file when its read fails', (t) => {
  const fs = require('node:fs');
  const { writeFencedRuleset } = require('../bin/lib/fence');
  const target = under(tempDir(t), 'AGENTS.md');
  const original = 'my existing instructions\n';
  writeText(target, original);
  const read = fs.readFileSync;
  const denied = Object.assign(new Error('read denied'), { code: 'EACCES' });
  const patched = t.mock.method(fs, 'readFileSync', function (file, ...args) {
    if (file === target) throw denied;
    return read.call(this, file, ...args);
  });
  assert.throws(() => writeFencedRuleset(target, {}, () => {}), (err) => err === denied);
  patched.mock.restore();
  assert.equal(readText(target), original);
});

test('Copilot CLI dry-run describes all files and writes nothing', (t) => {
  const root = tempDir(t);
  const home = under(root, 'home');
  const copilot = under(root, 'copilot-home');
  makeDir(home);

  const result = runCLI(['--only', 'copilot-cli', '--dry-run'], {
    cwd: root,
    env: isolatedEnv(home, { COPILOT_HOME: copilot }),
  });
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /would write puritanical ruleset to/);
  assert.match(result.out, /would copy compressor to/);
  assert.match(result.out, /would write postToolUse hook/);
  assert.match(result.out, /installed:\s+copilot-cli/);
  assert.equal(exists(copilot), false);
});

test('Claude uninstall removes an installed plugin through the Claude CLI', (t) => {
  const { root, home, cfg, bin, log } = fakeClaude(t, 'claude-uninstall.log', [
    'echo "$@" >> "$FAKE_LOG"',
    'if [ "$1" = "plugin" ] && [ "$2" = "list" ]; then echo puritanical; fi',
    'exit 0',
  ], [
    'echo %*>>"%FAKE_LOG%"',
    'if "%1 %2"=="plugin list" echo puritanical',
    'exit /b 0',
  ]);

  const result = runCLI(['--uninstall', '--only', 'claude', '--config-dir', cfg], {
    cwd: root,
    env: isolatedEnv(home, {
      PATH: bin + path.delimiter + process.env.PATH,
      FAKE_LOG: log,
    }),
  });
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /Uninstalled\. 1 item\(s\) cleaned/);
  const calls = readText(log);
  assert.match(calls, /plugin list/);
  assert.match(calls, /plugin uninstall puritanical@puritanical/);
});

test('hook validation drops unsupported hook types while retaining valid commands', () => {
  const settings = {
    hooks: {
      PostToolUse: [{
        hooks: [
          { type: 'prompt', prompt: 'unsupported' },
          { type: 'command', command: 'node valid-hook.js' },
        ],
      }],
    },
  };
  const returned = SETTINGS.validateHookFields(settings);
  assert.equal(returned, settings);
  assert.deepEqual(settings, {
    hooks: {
      PostToolUse: [{ hooks: [{ type: 'command', command: 'node valid-hook.js' }] }],
    },
  });
});

test('tools rejects unknown arguments with its usage message', (t) => {
  const dir = tempDir(t, 'puritanical-tools-coverage-');
  assert.throws(
    () => TOOLS.main(['--unknown-option'], dir, () => {}),
    /usage: npx github:yungSNAT\/Puritanical tools/,
  );
  assert.deepEqual(listDir(dir), []);
});

test('tools report prints an apply command including requested additional tools without changing files', (t) => {
  const dir = tempDir(t, 'puritanical-tools-coverage-');
  const project = under(dir, 'projects', 'project-a');
  makeDir(project, { recursive: true });

  const transcript = [
    JSON.stringify({
      type: 'attachment',
      attachment: {
        type: 'prompt_snapshot',
        tools: [{ name: 'Artifact', description: 'x'.repeat(5000), schema: {} }],
      },
    }),
    JSON.stringify({
      type: 'assistant',
      message: { id: 'request-1', usage: { input_tokens: 1 }, content: [] },
    }),
  ].join('\n') + '\n';
  const transcriptPath = under(project, 'session.jsonl');
  writeText(transcriptPath, transcript);
  const before = readText(transcriptPath);

  const output = [];
  TOOLS.main(['--also', 'Artifact'], dir, (line) => output.push(line));
  const rendered = output.join('\n');
  assert.match(rendered, /Artifact.*unused → deny/);
  assert.match(
    rendered,
    /apply with: npx github:yungSNAT\/Puritanical tools trim --also Artifact\s+\(undo: npx github:yungSNAT\/Puritanical tools restore\)/,
  );
  assert.equal(exists(under(dir, 'settings.json')), false);
  assert.equal(exists(under(dir, '.puritanical-trimmed-tools.json')), false);
  assert.equal(readText(transcriptPath), before);
});
