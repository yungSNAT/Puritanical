#!/usr/bin/env node
/* global __dirname, globalThis, process, require -- Node.js runtime */
// Upgrades from Chisle, the name this package had before 4.0.0: what an old
// install left behind is moved, replaced or removed, never duplicated.
// Run: node --test tests/test_migrate.js

'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const {
  exists, linkStat, listDir, makeDir, makeLink, makeTempDir, readJson, readText, remove, under, writeText,
} = require('./helpers/sandbox');

const CLI = under(__dirname, '..', 'bin', 'install.js');
const ACTIVATE = under(__dirname, '..', 'hooks', 'puritanical-activate.js');
const CONFIG = require('../hooks/puritanical-config');
const { requestedMode } = require('../hooks/puritanical-mode');
const { latestVersion, majorUpdateNotice } = require('../hooks/puritanical-activate');
const POSIX = { skip: process.platform === 'win32' };

function put(file, text, options) {
  makeDir(path.dirname(file), { recursive: true });
  writeText(file, text, options);
}
const putJson = (file, value) => put(file, JSON.stringify(value));

// Points XDG_CONFIG_HOME at dir for this test process until the test ends.
function useConfigHome(t, dir) {
  const saved = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = dir;
  t.after(() => {
    if (saved === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = saved;
  });
}

// A private home with every agent root inside it. On POSIX the PATH holds only
// the stand-in CLIs, so no real agent CLI is reached.
function sandbox(t) {
  const root = makeTempDir('puritanical-migrate-');
  t.after(() => remove(root, { recursive: true, force: true }));
  const dir = (...p) => under(root, ...p);
  makeDir(dir('bin'));
  makeDir(dir('claude'));
  const env = {
    ...process.env,
    HOME: dir('home'), USERPROFILE: dir('home'), PURITANICAL_HOME: dir('home'),
    CLAUDE_CONFIG_DIR: dir('claude'), XDG_CONFIG_HOME: dir('xdg'), APPDATA: dir('xdg'),
    COPILOT_HOME: dir('copilot'), OPENCODE_CONFIG_DIR: dir('opencode'), CODEX_HOME: dir('codex'),
    PATH: process.platform === 'win32' ? process.env.PATH : dir('bin'),
    PURITANICAL_UPDATE_CHECK: '0', PURITANICAL_DEFAULT_MODE: undefined,
  };
  for (const key of Object.keys(env)) if (key.startsWith('CHISLE_')) delete env[key];
  // A stand-in CLI: logs its arguments, and prints `list` output from <name>.list.
  const fake = (name, listed) => {
    put(dir('bin', name), '#!/bin/sh\n' +
      `echo "$*" >> "${dir(name + '.log')}"\n` +
      `case "$*" in *list*) /bin/cat "${dir(name + '.list')}";; esac\nexit 0\n`, { mode: 0o755 });
    put(dir(name + '.list'), listed);
  };
  const cli = (args, extra = {}) => {
    try {
      return execFileSync(process.execPath, [CLI, ...args, '--no-color'], {
        cwd: extra.cwd || root, env: { ...env, ...extra.env }, encoding: 'utf8', timeout: 20000,
      });
    } catch (e) { return (e.stdout || '') + (e.stderr || ''); }
  };
  const hook = (extra = {}) => execFileSync(process.execPath, [ACTIVATE], {
    env: { ...env, ...extra }, input: '{"source":"startup"}', encoding: 'utf8', timeout: 10000,
  });
  const log = name => (exists(dir(name + '.log')) ? readText(dir(name + '.log')) : '');
  return { dir, fake, cli, hook, log };
}

// ── config module and mode parser ───────────────────────────────────────────

test('an old CHISLE_* setting applies only where the PURITANICAL_* one is unset', () => {
  const env = { CHISLE_COMPRESS: '0', CHISLE_HOME: '/old', PURITANICAL_HOME: '/new', PATH: '/bin' };
  assert.equal(CONFIG.aliasOldEnv(env), env);
  assert.deepEqual(env, {
    CHISLE_COMPRESS: '0', CHISLE_HOME: '/old', PURITANICAL_HOME: '/new', PATH: '/bin', PURITANICAL_COMPRESS: '0',
  });
});

test('old state entries take their new names unless the new one exists', t => {
  const d = sandbox(t).dir('claude');
  put(under(d, '.chisle-active'), 'on');
  putJson(under(d, '.chisle-compress-stats.json'), { savedChars: 1, events: 1 });
  putJson(under(d, '.puritanical-compress-stats.json'), { savedChars: 9, events: 9 });
  put(under(d, 'chisle-spill', 'raw.txt'), 'raw');
  put(under(d, '.chisle-update-check.json'), '{}');

  assert.equal(CONFIG.migrateOldState(d), 2);
  assert.equal(readText(under(d, '.puritanical-active')), 'on');
  assert.equal(readText(under(d, 'puritanical-spill', 'raw.txt')), 'raw');
  assert.equal(readJson(under(d, '.puritanical-compress-stats.json')).events, 9, 'an existing ledger was overwritten');
  assert.ok(exists(under(d, '.chisle-compress-stats.json')), 'the old ledger went although it could not move');
  assert.ok(exists(under(d, '.chisle-update-check.json')), 'the npm update cache was carried over');
  assert.equal(CONFIG.migrateOldState(d), 0, 'a second run moved something');
});

test('a dangling link at the new name blocks the move, and links move as links', POSIX, t => {
  const d = sandbox(t).dir('claude');
  makeLink(under(d, 'nowhere'), under(d, '.puritanical-active'));
  put(under(d, '.chisle-active'), 'on');
  makeLink(under(d, 'elsewhere'), under(d, '.chisle-session-turns'));
  assert.equal(CONFIG.migrateOldState(d), 1);
  assert.ok(exists(under(d, '.chisle-active')));
  assert.ok(linkStat(under(d, '.puritanical-session-turns')).isSymbolicLink());
});

test('the old config folder hands over its settings and goes once empty', t => {
  const s = sandbox(t);
  useConfigHome(t, s.dir('xdg'));
  putJson(s.dir('xdg', 'chisle', 'config.json'), { defaultMode: 'off' });
  putJson(s.dir('xdg', 'chisle', '.chisle-compress-stats.json'), { savedChars: 5, events: 1 });

  assert.equal(CONFIG.getOldConfigDir(), s.dir('xdg', 'chisle'));
  assert.equal(CONFIG.migrateOldConfig(), 2);
  assert.deepEqual(readJson(s.dir('xdg', 'puritanical', 'config.json')), { defaultMode: 'off' });
  assert.ok(exists(s.dir('xdg', 'puritanical', '.puritanical-compress-stats.json')));
  assert.ok(!exists(s.dir('xdg', 'chisle')), 'the empty old folder stayed');

  put(s.dir('xdg', 'chisle', 'mcp-proxy', 'chisle-mcp-proxy.js'), '');
  assert.equal(CONFIG.migrateOldConfig(), 0);
  assert.ok(exists(s.dir('xdg', 'chisle', 'mcp-proxy')), 'the old proxy went before its servers were rewired');
});

test("'stop chisle', 'chisle mode' and /chisle still switch the mode", () => {
  assert.equal(requestedMode('stop chisle'), 'off');
  assert.equal(requestedMode('chisle mode off'), 'off');
  assert.equal(requestedMode('chisle mode'), 'on');
  assert.equal(requestedMode('/chisle'), 'on');
  assert.equal(requestedMode('stop puritanical'), 'off');
  assert.equal(requestedMode('chisled oak'), null);
});

// ── Claude Code hook ────────────────────────────────────────────────────────

test('the session hook reads an old CHISLE_* variable', t => {
  assert.equal(sandbox(t).hook({ CHISLE_DEFAULT_MODE: 'off' }), 'OK');
});

test('the session hook moves old state and config before reading the mode', t => {
  const s = sandbox(t);
  put(s.dir('claude', '.chisle-active'), 'on');
  putJson(s.dir('xdg', 'chisle', 'config.json'), { defaultMode: 'off' });
  assert.equal(s.hook(), 'OK', 'the mode did not come from the moved config');
  assert.ok(exists(s.dir('xdg', 'puritanical', 'config.json')));
  assert.ok(!exists(s.dir('claude', '.chisle-active')) && !exists(s.dir('claude', '.puritanical-active')));
});

test('the session hook asks for a new status badge while the old script is configured', t => {
  const s = sandbox(t);
  putJson(s.dir('claude', 'settings.json'), { statusLine: { type: 'command', command: 'bash "/x/chisle-hooks/chisle-statusline.sh"' } });
  assert.match(s.hook(), /STATUSLINE SETUP NEEDED[\s\S]*puritanical-statusline\.sh/);
  putJson(s.dir('claude', 'settings.json'), { statusLine: { type: 'command', command: 'my-status' } });
  assert.doesNotMatch(s.hook(), /STATUSLINE SETUP NEEDED/);
});

test('the update check asks the fork for its latest release', async t => {
  const s = sandbox(t);
  const calls = [];
  t.mock.method(globalThis, 'fetch', async (url, init) => {
    calls.push([url, init.headers]);
    return { ok: true, json: async () => ({ tag_name: calls.length === 1 ? 7 : 'v5.0.0' }) };
  });
  assert.equal(await latestVersion(s.dir('claude', 'a.json')), null, 'a tag that is not a string was accepted');
  assert.equal(await latestVersion(s.dir('claude', 'b.json')), '5.0.0');
  assert.equal(calls[0][0], 'https://api.github.com/repos/yungSNAT/Puritanical/releases/latest');
  assert.equal(calls[0][1]['User-Agent'], 'puritanical-update-check');
  assert.match(majorUpdateNotice('4.0.0', '5.0.0'),
    /claude plugin update puritanical@puritanical`, or rerun the installer with `npx github:yungSNAT\/Puritanical --update`/);
});

// ── installer: Claude Code ──────────────────────────────────────────────────

const MINE = [{ hooks: [{ type: 'command', command: 'mine' }] }];
const OLD_SETTINGS = {
  hooks: {
    SessionStart: [{ hooks: [{ type: 'command', command: 'node "/c/chisle-hooks/chisle-activate.js"' }] }],
    Stop: MINE,
  },
  statusLine: { type: 'command', command: 'bash "/c/chisle-hooks/chisle-statusline.sh"' },
};

test('the plugin path replaces chisle@chisle and drops old standalone wiring', POSIX, t => {
  const s = sandbox(t);
  s.fake('claude', 'chisle@chisle  3.14.2\n');
  putJson(s.dir('claude', 'settings.json'), OLD_SETTINGS);
  makeDir(s.dir('claude', 'chisle-hooks'));

  const dry = s.cli(['--only', 'claude', '--dry-run']);
  assert.match(dry, /would run: claude plugin uninstall chisle@chisle/);
  assert.match(dry, /would run: claude plugin marketplace remove chisle/);
  assert.doesNotMatch(s.log('claude'), /uninstall|remove/, 'a dry run removed the old plugin');
  assert.deepEqual(readJson(s.dir('claude', 'settings.json')), OLD_SETTINGS, 'a dry run changed settings');

  assert.match(s.cli(['--only', 'claude', '--update']), /Puritanical updater[\s\S]*Claude Code detected/);
  const calls = s.log('claude');
  assert.match(calls, /^plugin uninstall chisle@chisle\nplugin marketplace remove chisle$/m);
  assert.match(calls, /^plugin install puritanical@puritanical$/m);
  assert.deepEqual(readJson(s.dir('claude', 'settings.json')), { hooks: { Stop: MINE } });
  assert.ok(!exists(s.dir('claude', 'chisle-hooks')));
});

test('standalone wiring replaces old hook entries and an old status badge', POSIX, t => {
  const s = sandbox(t);
  putJson(s.dir('claude', 'settings.json'), OLD_SETTINGS);
  makeDir(s.dir('claude', 'chisle-hooks'));
  const out = s.cli(['--only', 'claude', '--update']);
  assert.match(out, /hooks wired/);
  assert.match(out, /statusline badge configured/);
  const settings = readJson(s.dir('claude', 'settings.json'));
  const commands = JSON.stringify(settings.hooks);
  assert.doesNotMatch(commands, /chisle-/);
  assert.match(commands, /puritanical-activate\.js/);
  assert.match(commands, /"mine"/);
  assert.match(settings.statusLine.command, /puritanical-hooks.puritanical-statusline\.sh/);
  assert.ok(!exists(s.dir('claude', 'chisle-hooks')));
  assert.ok(exists(s.dir('claude', 'puritanical-hooks', 'puritanical-activate.js')));
});

test('uninstall removes the old plugin, state and settings with the new', POSIX, t => {
  const s = sandbox(t);
  s.fake('claude', 'puritanical@puritanical\nchisle@chisle\n');
  putJson(s.dir('claude', 'settings.json'), OLD_SETTINGS);
  for (const f of ['.chisle-active', '.puritanical-active', '.chisle-update-check.json']) put(s.dir('claude', f), 'on');
  makeDir(s.dir('claude', 'chisle-spill'));
  assert.match(s.cli(['--uninstall', '--only', 'claude']), /Uninstalled/);
  assert.match(s.log('claude'), /^plugin uninstall puritanical@puritanical\nplugin list\nplugin uninstall chisle@chisle$/m);
  assert.deepEqual(readJson(s.dir('claude', 'settings.json')), { hooks: { Stop: MINE } });
  assert.deepEqual(listDir(s.dir('claude')).filter(f => !f.startsWith('settings.json')), []);
});

test('tools restore finds a trim record under its old name', t => {
  const s = sandbox(t);
  putJson(s.dir('claude', 'settings.json'), { permissions: { deny: ['Bash', 'WebFetch'] } });
  putJson(s.dir('claude', '.chisle-trimmed-tools.json'), { added: ['WebFetch'] });
  execFileSync(process.execPath, [CLI, 'tools', 'restore', '--config-dir', s.dir('claude')], { encoding: 'utf8' });
  assert.deepEqual(readJson(s.dir('claude', 'settings.json')), { permissions: { deny: ['Bash'] } });
  assert.ok(!exists(s.dir('claude', '.chisle-trimmed-tools.json')));
});

// ── installer: other agents ─────────────────────────────────────────────────

test('Gemini CLI and pi lose their old sources before the new ones go in', POSIX, t => {
  const s = sandbox(t);
  s.fake('gemini', 'chisle (3.14.2)\n');
  s.fake('pi', 'User packages:\n  npm:chisle@3.14.2\n  git:github.com/yungSNAT/Puritanical\n');
  const out = s.cli(['--only', 'gemini', '--only', 'pi', '--update']);
  assert.match(out, /Gemini CLI detected/, 'an old Gemini extension did not count as installed');
  assert.match(s.log('gemini'), /^extensions uninstall chisle\nextensions install https:\/\/github\.com\/yungSNAT\/Puritanical$/m);
  assert.match(s.log('pi'), /^remove npm:chisle\ninstall git:github\.com\/yungSNAT\/Puritanical$/m);
  assert.doesNotMatch(s.log('pi'), /remove git:/);
});

test('a fenced block with the old markers is replaced, not duplicated', t => {
  const s = sandbox(t);
  const file = s.dir('home', '.codex', 'AGENTS.md');
  put(file, '# mine\n\n<!-- chisle-begin -->\nold rules\n<!-- chisle-end -->\n\n# also mine\n');
  // The home comes from CHISLE_HOME alone, so the old variable is proved to apply.
  const elsewhere = { HOME: s.dir('elsewhere'), USERPROFILE: s.dir('elsewhere'), PURITANICAL_HOME: undefined, CHISLE_HOME: s.dir('home') };
  const out = s.cli(['--only', 'codex', '--update'], { env: elsewhere });
  assert.match(out, /replaced the pre-4\.0\.0 ruleset/);
  const text = readText(file);
  assert.equal(text.match(/<!-- puritanical-begin -->/g).length, 1);
  assert.doesNotMatch(text, /chisle-begin|old rules/);
  assert.match(text, /^# mine\n\n<!-- puritanical-begin -->[\s\S]*<!-- puritanical-end -->\n\n# also mine\n$/);

  put(file, 'mine\n<!-- chisle-begin -->\nold\n<!-- chisle-end -->\n');
  s.cli(['--uninstall', '--only', 'codex']);
  assert.equal(readText(file), 'mine\n');
});

test('OpenCode, Hermes and Copilot CLI drop old skills, plugins and hooks', t => {
  const s = sandbox(t);
  const oc = s.dir('opencode');
  const hermes = s.dir('home', '.hermes', 'skills');
  for (const dir of [under(oc, 'skills', 'chisle'), under(hermes, 'chisle'), under(hermes, 'chisle-help')]) {
    put(under(dir, 'SKILL.md'), 'old');
  }
  put(under(oc, 'skills', 'mine', 'SKILL.md'), 'mine');
  put(under(oc, 'plugins', 'chisle.js'), 'old');
  makeDir(under(oc, 'plugins', 'chisle-hooks'));
  put(s.dir('copilot', 'hooks', 'chisle.json'), '{}');
  makeDir(s.dir('copilot', 'chisle-hooks'));

  const out = s.cli(['--only', 'opencode', '--only', 'hermes', '--only', 'copilot-cli', '--update']);
  for (const agent of ['OpenCode', 'Hermes Agent', 'Copilot CLI']) assert.match(out, new RegExp(agent + ' detected'));
  assert.ok(!exists(under(oc, 'skills', 'chisle')) && exists(under(oc, 'skills', 'puritanical', 'SKILL.md')));
  assert.ok(exists(under(oc, 'skills', 'mine', 'SKILL.md')), 'a skill that is not ours went');
  assert.ok(!exists(under(oc, 'plugins', 'chisle.js')) && !exists(under(oc, 'plugins', 'chisle-hooks')));
  assert.ok(!exists(under(hermes, 'chisle')) && !exists(under(hermes, 'chisle-help')));
  assert.ok(exists(under(hermes, 'puritanical-help', 'SKILL.md')));
  assert.ok(!exists(s.dir('copilot', 'hooks', 'chisle.json')) && !exists(s.dir('copilot', 'chisle-hooks')));
  assert.ok(exists(s.dir('copilot', 'hooks', 'puritanical.json')));

  put(under(oc, 'skills', 'chisle-audit', 'SKILL.md'), 'old');
  put(under(oc, 'plugins', 'chisle', 'index.js'), 'old');
  put(s.dir('copilot', '.chisle-compress-stats.json'), '{}');
  s.cli(['--uninstall', '--only', 'opencode', '--only', 'copilot-cli']);
  assert.ok(!exists(under(oc, 'skills', 'chisle-audit')) && !exists(under(oc, 'skills', 'puritanical')));
  assert.ok(!exists(under(oc, 'plugins', 'chisle')));
  assert.ok(!exists(s.dir('copilot', '.chisle-compress-stats.json')) && !exists(s.dir('copilot', '.puritanical-compress-stats.json')));
});

test('an old project rule goes while it is the generated copy, and stays once edited', t => {
  const s = sandbox(t);
  const proj = s.dir('project');
  const old = under(proj, '.cursor', 'rules', 'chisle.mdc');
  put(old, '---\nalwaysApply: true\n---\n\n# Chisle\n\nrules\n');
  assert.match(s.cli(['--only', 'cursor', '--update'], { cwd: proj }), /removed the pre-4\.0\.0 rule/);
  assert.ok(!exists(old));
  assert.ok(exists(under(proj, '.cursor', 'rules', 'puritanical.mdc')));

  put(old, 'my own cursor rules\n');
  assert.match(s.cli(['--only', 'cursor', '--force'], { cwd: proj }), /left .*chisle\.mdc/);
  assert.ok(exists(old));
});

test('MCP servers wrapped by the old proxy move to the new one; uninstall unwraps both', POSIX, t => {
  const s = sandbox(t);
  const oldProxy = s.dir('xdg', 'chisle', 'mcp-proxy', 'chisle-mcp-proxy.js');
  const newProxy = s.dir('xdg', 'puritanical', 'mcp-proxy', 'puritanical-mcp-proxy.js');
  put(oldProxy, '');
  const config = s.dir('home', '.cursor', 'mcp.json');
  const wrapped = proxy => ({ mcpServers: { files: { command: 'node', args: [proxy, '--', 'files-server', '--root', '.'] } } });
  putJson(config, wrapped(oldProxy));

  s.cli(['--only', 'cline', '--dry-run'], { cwd: s.dir('home') });
  assert.deepEqual(readJson(config), wrapped(oldProxy), 'a dry run rewired a server');
  s.cli(['--only', 'cline'], { cwd: s.dir('home') });
  assert.deepEqual(readJson(config), wrapped(newProxy));
  assert.ok(exists(newProxy));
  assert.ok(exists(oldProxy), 'the old proxy went while servers wrapped with --file may use it');

  putJson(config, wrapped(oldProxy));
  assert.match(s.cli(['--uninstall']), /restored 1 MCP server/);
  assert.deepEqual(readJson(config), { mcpServers: { files: { command: 'files-server', args: ['--root', '.'] } } });
  assert.ok(!exists(s.dir('xdg', 'chisle', 'mcp-proxy')) && !exists(s.dir('xdg', 'puritanical', 'mcp-proxy')));
});

// ── pi extension ────────────────────────────────────────────────────────────

function piHarness(entries) {
  const handlers = new Map();
  const statuses = [];
  require('../pi-extension')({
    on: (name, fn) => handlers.set(name, fn), registerCommand() {}, appendEntry() {},
    getAllTools: () => [], events: { on: () => () => {}, emit() {} },
  });
  const ctx = {
    hasUI: true,
    ui: { theme: { fg: (_c, text) => text }, setStatus: (k, v) => statuses.push([k, v]), notify() {} },
    sessionManager: { getBranch: () => entries, buildContextEntries: () => entries },
  };
  return { handlers, statuses, ctx };
}

test('the pi extension moves an old config and honours old session entries', async t => {
  const s = sandbox(t);
  useConfigHome(t, s.dir('xdg'));
  putJson(s.dir('xdg', 'chisle', 'config.json'), { defaultMode: 'on' });

  const off = piHarness([{ type: 'custom', customType: 'chisle-mode', data: { mode: 'off' } }]);
  assert.ok(exists(s.dir('xdg', 'puritanical', 'config.json')));
  await off.handlers.get('session_start')({ type: 'session_start' }, off.ctx);
  assert.deepEqual(off.statuses.at(-1), ['puritanical', undefined], 'an old mode entry was ignored');

  const rules = piHarness([{ type: 'custom_message', customType: 'chisle-rules', content: 'rules' }]);
  await rules.handlers.get('session_start')({ type: 'session_start' }, rules.ctx);
  assert.equal(await rules.handlers.get('before_agent_start')({ systemPrompt: 'base' }, rules.ctx), undefined,
    'rules injected again beside an old rules entry');
});
