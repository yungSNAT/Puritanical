#!/usr/bin/env node
/* global process, require -- Node.js runtime */
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const {
  fs, exists, listDir, makeDir, readJson, readText, repoPath, restoreEnvironment,
  saveEnvironment, tempRoot, under, writeJson, writeText,
} = require('./helpers/sandbox');

const { INSTALLERS } = require('../bin/lib/agents');
const { piSources } = require('../bin/lib/uninstall');
const { writeAtomically } = require('../bin/lib/settings');
const { exitWhenHostGone } = require('../hooks/chisle-mcp-proxy');

const CLI = repoPath('bin', 'install.js');
const RECORD = '.chisle-trimmed-tools.json';

function runCLI(args, home) {
  const emptyPath = under(home, 'empty-path');
  makeDir(emptyPath, { recursive: true });
  const result = spawnSync(process.execPath, [CLI, ...args, '--no-color'], {
    cwd: home,
    env: {
      ...process.env,
      HOME: home,
      USERPROFILE: home,
      XDG_CONFIG_HOME: under(home, 'xdg'),
      CHISLE_HOME: home,
      PATH: emptyPath,
      NO_COLOR: '1',
      CI: '1',
    },
    input: '',
    encoding: 'utf8',
    timeout: 15000,
  });
  return { status: result.status, out: String(result.stdout || '') + String(result.stderr || '') };
}

function uninstallClaude(cfg, home) {
  return runCLI(['--uninstall', '--only', 'claude', '--config-dir', cfg], home);
}

function withLinkFailure(code, fn) {
  const originalLink = fs.linkSync;
  fs.linkSync = () => {
    const error = new Error(`link not supported (${code})`);
    error.code = code;
    throw error;
  };
  try {
    return fn();
  } finally {
    fs.linkSync = originalLink;
  }
}

test('Codex install records a write failure with the error message', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-codex-');
  writeText(under(home, '.codex'), 'a file where the directory should be');
  const saved = saveEnvironment(['CHISLE_HOME']);
  process.env.CHISLE_HOME = home;
  const results = { detected: 0, installed: [], skipped: [], failed: [] };
  const originalWrite = process.stdout.write;
  process.stdout.write = () => true;
  try {
    INSTALLERS.get('codex')({
      say() {}, note() {}, warn() {},
      opts: { force: false, dryRun: false },
      results,
    });
  } finally {
    process.stdout.write = originalWrite;
    restoreEnvironment(saved);
  }
  assert.deepEqual(results.installed, []);
  assert.equal(results.failed.length, 1);
  assert.equal(results.failed[0][0], 'codex');
  assert.notEqual(results.failed[0][1], 'write failed');
  assert.ok(results.failed[0][1].length > 0);
});

test('Claude uninstall removes only the tool deny entries chisle added', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-tools-');
  const cfg = under(home, 'claude');
  makeDir(cfg);
  writeJson(under(cfg, 'settings.json'), { permissions: { deny: ['Workflow', 'Bash(rm -rf:*)'] } });
  writeJson(under(cfg, RECORD), { added: ['Workflow'] });

  const result = uninstallClaude(cfg, home);
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /removed tool deny entries added by chisle: Workflow/);
  assert.deepEqual(readJson(under(cfg, 'settings.json')).permissions, { deny: ['Bash(rm -rf:*)'] });
  assert.equal(exists(under(cfg, RECORD)), false);
});

test('Claude uninstall warns and leaves unparseable settings alone when restoring tools', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-badtools-');
  const cfg = under(home, 'claude');
  makeDir(cfg);
  writeText(under(cfg, 'settings.json'), '{ not json ::: }');
  writeJson(under(cfg, RECORD), { added: ['Workflow'] });

  const result = uninstallClaude(cfg, home);
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /is not valid JSON; not touching it/);
  assert.equal(exists(under(cfg, RECORD)), true);
});

test('MCP proxy stops the server and exits cleanly when the host stops reading', () => {
  const originalExit = process.exit;
  const exits = [];
  process.exit = (code) => { exits.push(code); };
  let kills = 0;
  try {
    exitWhenHostGone({ kill() { kills++; } })();
    exitWhenHostGone({ kill() { throw new Error('already gone'); } })();
  } finally {
    process.exit = originalExit;
  }
  assert.equal(kills, 1);
  assert.deepEqual(exits, [0, 0]);
});

test('pi sources ignore text after the source and refuse unsafe characters', () => {
  const output = [
    'npm:chisle@3.0.0 (user)',
    '  git:https://github.com/JayPokale/Chisle@v1.2.0   (project)',
    'npm:chisle',
    'npm:chisle@1;calc (user)',
    'npm:other@1.0.0 (user)',
  ].join('\n');
  assert.deepEqual(piSources(output), [
    'npm:chisle',
    'git:https://github.com/JayPokale/Chisle',
    'npm:chisle',
  ]);
});

test('backups fall back to an exclusive copy where hard links are unsupported', (t) => {
  const dir = tempRoot(t, 'chisle-cov-f-nolink-');
  const backup = under(dir, 'mcp.json.chisle-bak');
  withLinkFailure('EPERM', () => writeAtomically(backup, 'first', { noReplace: true }));
  assert.equal(readText(backup), 'first');
  assert.deepEqual(listDir(dir), ['mcp.json.chisle-bak']);

  assert.throws(
    () => withLinkFailure('ENOTSUP', () => writeAtomically(backup, 'second', { noReplace: true })),
    error => error.code === 'EEXIST',
  );
  assert.equal(readText(backup), 'first');
  assert.deepEqual(listDir(dir), ['mcp.json.chisle-bak']);
});

test('update repairs an MCP proxy installed without the command filters', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-proxy-');
  const proxy = under(home, 'xdg', 'chisle', 'mcp-proxy');
  makeDir(proxy, { recursive: true });
  writeText(under(proxy, 'chisle-mcp-proxy.js'), '// stale copy\n');

  const result = runCLI(['--update'], home);
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /MCP proxy refreshed in/);
  assert.equal(exists(under(proxy, 'chisle-command-filters.js')), true);
  assert.equal(readText(under(proxy, 'chisle-mcp-proxy.js')), readText(repoPath('hooks', 'chisle-mcp-proxy.js')));
});

test('install leaves the MCP proxy alone when none is installed and warns when a refresh fails', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-noproxy-');
  let result = runCLI(['--update'], home);
  assert.equal(result.status, 0, result.out);
  assert.doesNotMatch(result.out, /MCP proxy/);
  assert.equal(exists(under(home, 'xdg', 'chisle', 'mcp-proxy')), false);

  makeDir(under(home, 'xdg', 'chisle', 'mcp-proxy', 'chisle-config.js'), { recursive: true });
  result = runCLI(['--update'], home);
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /MCP proxy refresh failed/);
});

test('update replaces a planted proxy-file link and leaves its target untouched', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-proxylink-');
  const proxy = under(home, 'xdg', 'chisle', 'mcp-proxy');
  const sentinel = under(home, 'sentinel.txt');
  makeDir(proxy, { recursive: true });
  writeText(sentinel, 'do not touch\n');
  fs.symlinkSync(sentinel, under(proxy, 'chisle-mcp-proxy.js')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

  const result = runCLI(['--update'], home);
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /MCP proxy refreshed in/);
  assert.equal(readText(sentinel), 'do not touch\n');
  const st = fs.lstatSync(under(proxy, 'chisle-mcp-proxy.js')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  assert.equal(st.isFile() && !st.isSymbolicLink(), true);
  assert.equal(readText(under(proxy, 'chisle-mcp-proxy.js')), readText(repoPath('hooks', 'chisle-mcp-proxy.js')));
});

test('update refuses an MCP proxy folder that is a link', (t) => {
  const home = tempRoot(t, 'chisle-cov-f-proxydir-');
  const elsewhere = under(home, 'elsewhere');
  makeDir(elsewhere, { recursive: true });
  makeDir(under(home, 'xdg', 'chisle'), { recursive: true });
  fs.symlinkSync(elsewhere, under(home, 'xdg', 'chisle', 'mcp-proxy'), 'dir'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

  const result = runCLI(['--update'], home);
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /MCP proxy refresh failed: unsafe MCP proxy folder/);
  assert.deepEqual(listDir(elsewhere), []);
});
