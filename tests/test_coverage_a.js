'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const Module = require('module');

const CLI = path.join(__dirname, '..', 'bin', 'install.js');
const SETTINGS = require('../bin/lib/settings');
const TOOLS = require('../bin/lib/tools');

function tempDir(t, prefix = 'chisle-coverage-a-') {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function isolatedEnv(home, extra = {}) {
  return {
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: path.join(home, 'xdg'),
    CHISLE_HOME: home,
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
  fs.mkdirSync(binDir, { recursive: true });
  if (process.platform === 'win32') {
    const file = path.join(binDir, `${name}.cmd`);
    fs.writeFileSync(file, `@echo off\r\n${windowsBody.replace(/\n/g, '\r\n')}\r\n`);
    return file;
  }
  const file = path.join(binDir, name);
  fs.writeFileSync(file, `#!/bin/sh\n${posixBody}\n`, { mode: 0o755 });
  return file;
}

function withEnv(changes, fn) {
  const previous = new Map();
  for (const [key, value] of Object.entries(changes)) {
    previous.set(key, Object.prototype.hasOwnProperty.call(process.env, key)
      ? process.env[key]
      : undefined);
    if (value === undefined) delete process.env[key];
    else process.env[key] = String(value);
  }
  try {
    return fn();
  } finally {
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

function loadInstallerInternals(platform) {
  delete require.cache[require.resolve(CLI)];
  let source = fs.readFileSync(CLI, 'utf8').replace(/^#!/, '//');
  const replacement = [
    'module.exports = {',
    '  quoteWin, claudeDir, installedFor, installClaude,',
    '  setInstallClaudeHooks: (fn) => { installClaudeHooks = fn; },',
    '};',
  ].join(' ');
  assert.match(source, /\nmain\(\);\s*$/);
  source = source.replace(/\nmain\(\);\s*$/, `\n${replacement}\n`);

  const descriptor = Object.getOwnPropertyDescriptor(process, 'platform');
  Object.defineProperty(process, 'platform', {
    configurable: true,
    enumerable: descriptor.enumerable,
    value: platform,
  });
  try {
    const mod = new Module(CLI, module);
    mod.filename = CLI;
    mod.paths = Module._nodeModulePaths(path.dirname(CLI));
    mod._compile(source, CLI);
    return mod.exports;
  } finally {
    Object.defineProperty(process, 'platform', descriptor);
  }
}

test('Windows command quoting quotes empty and spaced arguments while preserving plain arguments', () => {
  const windowsInstaller = loadInstallerInternals('win32');
  assert.equal(windowsInstaller.quoteWin(''), '""');
  assert.equal(windowsInstaller.quoteWin('two words'), '"two words"');
  assert.equal(windowsInstaller.quoteWin('a"b'), '"a\\"b"');
  assert.equal(windowsInstaller.quoteWin('plain'), 'plain');

  const nonWindowsInstaller = loadInstallerInternals('linux');
  assert.equal(nonWindowsInstaller.quoteWin('two words'), 'two words');
});

test('default Claude directory uses the isolated home and foreign hooks are not treated as installed', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const emptyPath = path.join(root, 'empty-path');
  const cfg = path.join(home, '.claude');
  const settingsPath = path.join(cfg, 'settings.json');
  fs.mkdirSync(cfg, { recursive: true });
  fs.mkdirSync(emptyPath);
  fs.writeFileSync(settingsPath, JSON.stringify({
    hooks: {
      SessionStart: [{
        hooks: [{ type: 'command', command: 'node foreign-startup.js' }],
      }],
    },
  }));

  const installer = loadInstallerInternals(process.platform);
  withEnv({
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: path.join(home, 'xdg'),
    CHISLE_HOME: undefined,
    CLAUDE_CONFIG_DIR: undefined,
    PATH: emptyPath,
  }, () => {
    assert.equal(installer.claudeDir({}), cfg);
    assert.equal(installer.installedFor('claude', {}), false);

    fs.writeFileSync(settingsPath, JSON.stringify({
      hooks: {
        SessionStart: [{
          hooks: [{ type: 'command', command: 'node chisle-activate.js' }],
        }],
      },
    }));
    assert.equal(installer.installedFor('claude', {}), true);
  });
});

test('Claude standalone skip result is recorded as skipped rather than installed or failed', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const emptyPath = path.join(root, 'empty-path');
  fs.mkdirSync(home);
  fs.mkdirSync(emptyPath);

  const installer = loadInstallerInternals(process.platform);
  installer.setInstallClaudeHooks(() => 'skip');

  const results = { detected: 0, installed: [], skipped: [], failed: [] };
  const notes = [];
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function capture(chunk) {
    writes.push(String(chunk));
    return true;
  };

  try {
    withEnv({
      HOME: home,
      USERPROFILE: home,
      XDG_CONFIG_HOME: path.join(home, 'xdg'),
      CLAUDE_CONFIG_DIR: undefined,
      PATH: emptyPath,
    }, () => {
      installer.installClaude({
        opts: { force: false, dryRun: false, configDir: null },
        results,
        say: (s) => notes.push(s),
        note: (s) => notes.push(s),
        warn: (s) => notes.push(s),
      });
    });
  } finally {
    process.stdout.write = originalWrite;
  }

  assert.equal(results.detected, 1);
  assert.deepEqual(results.installed, []);
  assert.deepEqual(results.failed, []);
  assert.deepEqual(results.skipped, [['claude-hooks', 'already wired']]);
  assert.ok(notes.some((line) => /CLI not on PATH/.test(line)));
  assert.equal(writes.join(''), '\n');
});

function fakeClaude(t, logName, unixLines, windowsLines) {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const cfg = path.join(root, 'claude-config');
  const bin = path.join(root, 'bin');
  const log = path.join(root, logName);
  fs.mkdirSync(home);
  fs.mkdirSync(cfg);
  writeFakeCommand(bin, 'claude', unixLines.join('\n'), windowsLines.join('\n'));
  return { root, home, cfg, bin, log };
}

test('failed Claude plugin installation warns and falls back to working standalone hooks', (t) => {
  const { root, home, cfg, bin, log } = fakeClaude(
    t,
    'claude.log',
    [
      'echo "$@" >> "$FAKE_LOG"',
      'if [ "$1" = "plugin" ] && [ "$2" = "list" ]; then',
      '  echo none',
      '  exit 0',
      'fi',
      'exit 1',
    ],
    [
      'echo %*>>"%FAKE_LOG%"',
      'if "%1 %2"=="plugin list" (',
      '  echo none',
      '  exit /b 0',
      ')',
      'exit /b 1',
    ],
  );

  const env = isolatedEnv(home, {
    PATH: bin + path.delimiter + process.env.PATH,
    FAKE_LOG: log,
  });
  const result = runCLI(['--only', 'claude', '--config-dir', cfg], { cwd: root, env });

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /plugin install failed, falling back to standalone hooks/);
  assert.match(result.out, /installed:\s+claude-hooks/);

  const calls = fs.readFileSync(log, 'utf8');
  assert.match(calls, /plugin marketplace add JayPokale\/Chisle/);
  assert.match(calls, /plugin install chisle@chisle/);

  const settings = JSON.parse(fs.readFileSync(path.join(cfg, 'settings.json'), 'utf8'));
  assert.ok(settings.hooks.SessionStart);
  assert.ok(settings.hooks.PostToolUse);
  assert.ok(fs.existsSync(path.join(cfg, 'chisle-hooks', 'chisle-compress-output.js')));
});

test('unparseable Claude settings produce a failed install and preserve the original file', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const cfg = path.join(root, 'claude-config');
  const emptyPath = path.join(root, 'empty-path');
  fs.mkdirSync(home);
  fs.mkdirSync(cfg);
  fs.mkdirSync(emptyPath);

  const settingsPath = path.join(cfg, 'settings.json');
  const invalid = '{ this is not valid JSON';
  fs.writeFileSync(settingsPath, invalid);

  const result = runCLI(['--only', 'claude', '--config-dir', cfg], {
    cwd: root,
    env: isolatedEnv(home, { PATH: emptyPath }),
  });

  assert.equal(result.status, 1, result.out);
  assert.match(result.out, /settings\.json unparseable/);
  assert.match(result.out, /failed:\s+claude-hooks \(settings\.json unparseable\)/);
  assert.equal(fs.readFileSync(settingsPath, 'utf8'), invalid);
});

test('existing non-Chisle statusline is preserved and reported instead of overwritten', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const cfg = path.join(root, 'claude-config');
  const emptyPath = path.join(root, 'empty-path');
  fs.mkdirSync(home);
  fs.mkdirSync(cfg);
  fs.mkdirSync(emptyPath);

  const settingsPath = path.join(cfg, 'settings.json');
  fs.writeFileSync(settingsPath, JSON.stringify({
    model: 'opus',
    statusLine: { type: 'command', command: 'my-existing-statusline' },
  }));

  const result = runCLI(['--only', 'claude', '--config-dir', cfg], {
    cwd: root,
    env: isolatedEnv(home, { PATH: emptyPath }),
  });

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /existing statusline detected, CHISLE badge NOT added/);
  const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8'));
  assert.deepEqual(settings.statusLine, {
    type: 'command',
    command: 'my-existing-statusline',
  });
  assert.ok(settings.hooks.SessionStart);
});

test('Gemini reports an existing extension as skipped and does not reinstall it', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const bin = path.join(root, 'bin');
  const log = path.join(root, 'gemini.log');
  fs.mkdirSync(home);

  writeFakeCommand(
    bin,
    'gemini',
    [
      'echo "$@" >> "$FAKE_LOG"',
      'if [ "$1" = "extensions" ] && [ "$2" = "list" ]; then echo chisle; fi',
      'exit 0',
    ].join('\n'),
    [
      'echo %*>>"%FAKE_LOG%"',
      'if "%1 %2"=="extensions list" echo chisle',
      'exit /b 0',
    ].join('\n'),
  );

  const result = runCLI(['--only', 'gemini'], {
    cwd: root,
    env: isolatedEnv(home, {
      PATH: bin + path.delimiter + process.env.PATH,
      FAKE_LOG: log,
    }),
  });

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /chisle extension already installed/);
  assert.match(result.out, /skipped:\s+gemini/);
  const calls = fs.readFileSync(log, 'utf8');
  assert.match(calls, /extensions list/);
  assert.doesNotMatch(calls, /extensions install/);
});

test('Codex installation separates an unterminated user file from the fenced ruleset', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const target = path.join(home, '.codex', 'AGENTS.md');
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, 'user instructions without newline');

  const result = runCLI(['--only', 'codex'], {
    cwd: root,
    env: isolatedEnv(home),
  });

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /installed:/);
  const written = fs.readFileSync(target, 'utf8');
  assert.ok(written.startsWith(
    'user instructions without newline\n\n<!-- chisle-begin -->\n',
  ));
  assert.match(written, /<!-- chisle-end -->\n$/);
});

test('Copilot CLI dry-run describes all files and writes nothing', (t) => {
  const root = tempDir(t);
  const home = path.join(root, 'home');
  const copilot = path.join(root, 'copilot-home');
  fs.mkdirSync(home);

  const result = runCLI(['--only', 'copilot-cli', '--dry-run'], {
    cwd: root,
    env: isolatedEnv(home, { COPILOT_HOME: copilot }),
  });

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /would write chisle ruleset to/);
  assert.match(result.out, /would copy compressor to/);
  assert.match(result.out, /would write postToolUse hook/);
  assert.match(result.out, /installed:\s+copilot-cli/);
  assert.equal(fs.existsSync(copilot), false);
});

test('Claude uninstall removes an installed plugin through the Claude CLI', (t) => {
  const { root, home, cfg, bin, log } = fakeClaude(
    t,
    'claude-uninstall.log',
    [
      'echo "$@" >> "$FAKE_LOG"',
      'if [ "$1" = "plugin" ] && [ "$2" = "list" ]; then echo chisle; fi',
      'exit 0',
    ],
    [
      'echo %*>>"%FAKE_LOG%"',
      'if "%1 %2"=="plugin list" echo chisle',
      'exit /b 0',
    ],
  );

  const result = runCLI(
    ['--uninstall', '--only', 'claude', '--config-dir', cfg],
    {
      cwd: root,
      env: isolatedEnv(home, {
        PATH: bin + path.delimiter + process.env.PATH,
        FAKE_LOG: log,
      }),
    },
  );

  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /Uninstalled\. 1 item\(s\) cleaned/);
  const calls = fs.readFileSync(log, 'utf8');
  assert.match(calls, /plugin list/);
  assert.match(calls, /plugin uninstall chisle@chisle/);
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
      PostToolUse: [{
        hooks: [{ type: 'command', command: 'node valid-hook.js' }],
      }],
    },
  });
});

test('tools rejects unknown arguments with its usage message', (t) => {
  const dir = tempDir(t, 'chisle-tools-coverage-');
  assert.throws(
    () => TOOLS.main(['--unknown-option'], dir, () => {}),
    /usage: npx chisle tools/,
  );
  assert.deepEqual(fs.readdirSync(dir), []);
});

test('tools report prints an apply command including requested additional tools without changing files', (t) => {
  const dir = tempDir(t, 'chisle-tools-coverage-');
  const project = path.join(dir, 'projects', 'project-a');
  fs.mkdirSync(project, { recursive: true });

  const transcript = [
    JSON.stringify({
      type: 'attachment',
      attachment: {
        type: 'prompt_snapshot',
        tools: [{
          name: 'Artifact',
          description: 'x'.repeat(5000),
          schema: {},
        }],
      },
    }),
    JSON.stringify({
      type: 'assistant',
      message: {
        id: 'request-1',
        usage: { input_tokens: 1 },
        content: [],
      },
    }),
  ].join('\n') + '\n';
  const transcriptPath = path.join(project, 'session.jsonl');
  fs.writeFileSync(transcriptPath, transcript);
  const before = fs.readFileSync(transcriptPath, 'utf8');

  const output = [];
  TOOLS.main(['--also', 'Artifact'], dir, (line) => output.push(line));
  const rendered = output.join('\n');

  assert.match(rendered, /Artifact.*unused → deny/);
  assert.match(
    rendered,
    /apply with: npx chisle tools trim --also Artifact\s+\(undo: npx chisle tools restore\)/,
  );
  assert.equal(fs.existsSync(path.join(dir, 'settings.json')), false);
  assert.equal(fs.existsSync(path.join(dir, '.chisle-trimmed-tools.json')), false);
  assert.equal(fs.readFileSync(transcriptPath, 'utf8'), before);
});
