/* global __dirname, process, require -- Node.js runtime */
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync, spawnSync } = require('node:child_process');

const installer = path.resolve(__dirname, '../bin/install.js');
const activate = path.resolve(__dirname, '../hooks/puritanical-activate.js');

// Each test file has its own Node process. Relative fixture paths stay inside a
// fresh working directory, including when the installer's config path has spaces.
function isolatedConfig(t) {
  const previous = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical config '));
  process.chdir(root);
  t.after(() => {
    process.chdir(previous);
    fs.rmSync(root, { recursive: true, force: true });
  });
  fs.mkdirSync('empty-bin');
  fs.mkdirSync('claude');
  return {
    HOME: root, USERPROFILE: root, PURITANICAL_HOME: root,
    PATH: path.resolve('empty-bin'), CLAUDE_CONFIG_DIR: path.resolve('claude'),
    XDG_CONFIG_HOME: path.resolve('xdg'), COPILOT_HOME: path.resolve('copilot'),
    SystemRoot: process.env.SystemRoot, CI: 'true',
    NODE_V8_COVERAGE: process.env.NODE_V8_COVERAGE,
  };
}

test('update refreshes installed agents and preserves user rules and absent integrations', t => {
  const env = isolatedConfig(t);
  const run = (...args) => execFileSync(process.execPath, [installer, ...args, '--no-color'], {
    env, encoding: 'utf8', timeout: 10000,
  });
  fs.mkdirSync('.codex');
  fs.writeFileSync('.codex/AGENTS.md', 'Local instructions\n');

  const installedAgents = ['--only', 'claude', '--only', 'codex', '--only', 'hermes'];
  run(...installedAgents);
  fs.writeFileSync('.hermes/skills/puritanical/SKILL.md', 'outdated');
  run('--update', ...installedAgents, '--only', 'copilot-cli', '--only', 'gemini', '--only', 'opencode');

  assert.match(fs.readFileSync('.hermes/skills/puritanical/SKILL.md', 'utf8'), /name: puritanical/);
  const rules = fs.readFileSync('.codex/AGENTS.md', 'utf8');
  assert.ok(rules.startsWith('Local instructions\n'));
  assert.equal(rules.split('<!-- puritanical-begin -->').length, 2);
  assert.equal(fs.existsSync('copilot'), false);
  assert.equal(fs.existsSync('xdg/opencode'), false);
  assert.equal(fs.existsSync('.gemini'), false);
  const settings = JSON.parse(fs.readFileSync('claude/settings.json', 'utf8'));
  assert.equal(settings.hooks.SessionStart.length, 1);

  run('--uninstall', ...installedAgents);
  assert.equal(fs.readFileSync('.codex/AGENTS.md', 'utf8').trim(), 'Local instructions');
  assert.equal(fs.existsSync('claude/puritanical-hooks'), false);
  assert.equal(fs.existsSync('.hermes/skills/puritanical'), false);
});

test('tools reports empty sessions and rejects invalid options without writing configuration', t => {
  const env = isolatedConfig(t);
  const invoke = (...args) => spawnSync(process.execPath, [installer, 'tools', '--config-dir', 'claude', ...args], {
    env, encoding: 'utf8', timeout: 5000,
  });
  const empty = invoke();
  assert.equal(empty.status, 0);
  assert.match(empty.stdout, /no tool schemas or skill listings found in 0 session/);
  const invalid = invoke('--days', '0');
  assert.equal(invalid.status, 2);
  assert.match(invalid.stderr, /positive number/);
  assert.deepEqual(fs.readdirSync('claude'), []);
});

test('startup uses a cached major update offline and respects the configured statusline', t => {
  const env = isolatedConfig(t);
  fs.writeFileSync('claude/settings.json', JSON.stringify({ statusLine: { type: 'command', command: 'my-status' } }));
  fs.writeFileSync('claude/.puritanical-update-check.json', JSON.stringify({ latest: '99.0.0', checkedAt: Date.now() }));
  fs.writeFileSync('offline.cjs', 'globalThis.fetch = () => { throw Error("offline fixture"); };\n');
  const run = mode => execFileSync(process.execPath, ['--require', './offline.cjs', activate], {
    env: { ...env, PURITANICAL_DEFAULT_MODE: mode }, input: '{"source":"startup"}', encoding: 'utf8', timeout: 5000,
  });
  const output = run('on');
  assert.match(output, /UPDATE AVAILABLE: v99\.0\.0/);
  assert.doesNotMatch(output, /STATUSLINE SETUP NEEDED/);
  assert.equal(fs.readFileSync('claude/.puritanical-active', 'utf8'), 'on');
  assert.equal(run('off'), 'OK');
  assert.equal(fs.existsSync('claude/.puritanical-active'), false);
});

function interactiveInstaller(t) {
  const env = isolatedConfig(t);
  delete env.CI;
  // The child reads real answers from a pipe, with terminal detection enabled.
  // The empty PATH prevents any call to an installed GitHub client.
  fs.writeFileSync('terminal.cjs', 'process.stdin.isTTY = true; process.stdout.isTTY = true;\n');
  return (agent, answer) => execFileSync(process.execPath,
    ['--require', './terminal.cjs', installer, '--only', agent, '--no-color'], {
      env, input: answer + '\n', encoding: 'utf8', timeout: 5000,
    });
}

test('interactive installation remembers a refusal and asks again after a deferral', t => {
  const install = interactiveInstaller(t);
  fs.mkdirSync('xdg/puritanical', { recursive: true });
  fs.writeFileSync('xdg/puritanical/config.json', '{"sections":{"prose":false}}');
  const config = () => JSON.parse(fs.readFileSync('xdg/puritanical/config.json', 'utf8'));

  assert.match(install('cursor', ''), /ask again next time/);
  assert.deepEqual(config(), { sections: { prose: false } });
  assert.match(install('cline', 'n'), /won't ask again/);
  assert.deepEqual(config(), { sections: { prose: false }, starPrompt: 'no' });
  assert.doesNotMatch(install('kiro', 'y'), /Star and follow on GitHub/);
  assert.equal(config().starPrompt, 'no');
});

test('a yes stars and follows this fork through the gh on the PATH', { skip: process.platform === 'win32' }, t => {
  const install = interactiveInstaller(t);
  // A stand-in gh on the PATH: it logs each call and answers `api user` with a login.
  fs.writeFileSync('empty-bin/gh', [
    '#!/bin/sh',
    'echo "$*" >> "$HOME/gh.log"',
    '[ "$2" = user ] && echo someone',
    'exit 0',
    '',
  ].join('\n'), { mode: 0o755 });
  assert.match(install('cursor', 'y'), /Starred and following/);
  const calls = fs.readFileSync('gh.log', 'utf8');
  assert.match(calls, /^api -X PUT user\/starred\/yungSNAT\/Puritanical --silent$/m);
  assert.match(calls, /^api -X PUT user\/following\/yungSNAT --silent$/m);
});

test('interactive installation prints manual links offline and preserves unreadable settings', t => {
  const install = interactiveInstaller(t);
  fs.mkdirSync('xdg/puritanical', { recursive: true });
  fs.writeFileSync('xdg/puritanical/config.json', '{"mode":');
  const output = install('cursor', 'yes');
  assert.match(output, /Couldn't star through gh\. Star it here: https:\/\/github\.com\//);
  assert.match(output, /Follow here: https:\/\/github\.com\//);
  assert.doesNotMatch(output, /Starred/);
  assert.equal(fs.readFileSync('xdg/puritanical/config.json', 'utf8'), '{"mode":');
});
