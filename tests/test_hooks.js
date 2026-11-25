#!/usr/bin/env node
/* global __dirname, clearTimeout, process, require, setTimeout -- Node.js runtime */
// Basic hook tests — validates config logic and flag safety
// Run: node --test tests/test_hooks.js

const { test, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const {
  absolutePath, exists, makeDir, readJson, readText, remove, under, writeText,
} = require('./helpers/sandbox');
const fs = require('fs');
const isolateWorkingDirectory = require('./fixtures/working-directory');

const { VALID_MODES, safeWriteFlag, readFlag, readJsonFile, readSettingsText, getDefaultMode } = require('../hooks/puritanical-config');
const { symlinkSkip } = require('./fixtures/symlinks');

isolateWorkingDirectory();

beforeEach(t => {
  const previous = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = absolutePath('test-config');
  t.after(() => {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = previous;
  });
});

// ── VALID_MODES ──────────────────────────────────────────────────────────────

test('VALID_MODES contains on and off', () => {
  assert.ok(VALID_MODES.includes('off'));
  assert.ok(VALID_MODES.includes('on'));
  assert.equal(VALID_MODES.length, 2);
});

// ── safeWriteFlag / readFlag ─────────────────────────────────────────────────

test('safeWriteFlag writes and readFlag reads back', () => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const flagPath = under(dir, '.puritanical-active');
  safeWriteFlag(flagPath, 'on');
  assert.equal(readFlag(flagPath), 'on');
});

test('readFlag returns null for missing file', () => {
  assert.equal(readFlag('missing-flag'), null);
});

test('readFlag returns null for unknown mode', () => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const flagPath = under(dir, '.puritanical-active');
  writeText(flagPath, 'wenyan-ultra', { mode: 0o600 });
  assert.equal(readFlag(flagPath), null);
});

test('readFlag returns null for symlink', { skip: symlinkSkip }, () => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const flagPath = under(dir, '.puritanical-active');
  const target = under(dir, 'target');
  writeText(target, 'on', { mode: 0o600 });
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
  fs.symlinkSync(target, flagPath); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  assert.equal(readFlag(flagPath), null);
});

test('readFlag returns null for oversized file', () => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const flagPath = under(dir, '.puritanical-active');
  writeText(flagPath, 'on' + 'x'.repeat(100), { mode: 0o600 });
  assert.equal(readFlag(flagPath), null);
});

test('safeWriteFlag refuses to overwrite a symlink', { skip: symlinkSkip }, () => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const flagPath = under(dir, '.puritanical-active');
  const target = under(dir, 'target');
  writeText(target, '', { mode: 0o600 });
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
  fs.symlinkSync(target, flagPath); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  safeWriteFlag(flagPath, 'on'); // must not throw, must not overwrite
  assert.equal(readText(target), ''); // target untouched
});

test('cached JSON accepts the byte limit and rejects larger files and directories', () => {
  writeText('cache.json', '{"v":1}');
  assert.deepEqual(readJsonFile('cache.json', 7), { v: 1 });
  assert.equal(readJsonFile('cache.json', 6), null);
  assert.equal(readJsonFile('.', 1024), null);
});

test('cached JSON refuses symlinks', { skip: symlinkSkip }, () => {
  writeText('cache.json', '{"v":1}');
  fs.symlinkSync('cache.json', 'linked-cache.json');
  assert.equal(readJsonFile('linked-cache.json'), null);
});

test('settings reads skip oversized files and directories', () => {
  writeText('oversize.json', ' '.repeat(4 * 1024 * 1024 + 1));
  assert.equal(readSettingsText('oversize.json'), null);
  assert.equal(readSettingsText('.'), null);
});

test('settings reads accept user-managed dotfile links', { skip: symlinkSkip }, () => {
  writeText('dotfile.json', '{"statusLine":true}');
  fs.symlinkSync('dotfile.json', 'settings.json');
  assert.equal(readSettingsText('settings.json'), '{"statusLine":true}');
});

test('flag writes accept a configured directory link to a user-owned directory', { skip: symlinkSkip }, () => {
  makeDir('state');
  fs.symlinkSync('state', 'linked-state', 'dir');
  const flag = absolutePath('linked-state', '.puritanical-active');
  // Windows accepts linked state only under the configured home.
  const previous = process.env.USERPROFILE;
  try {
    if (process.platform === 'win32') process.env.USERPROFILE = process.cwd();
    safeWriteFlag(flag, 'on');
    assert.equal(readText('state/.puritanical-active'), 'on');
  } finally {
    if (previous === undefined) delete process.env.USERPROFILE;
    else process.env.USERPROFILE = previous;
  }
});

test('systems without a user ID limit linked state directories to the home', { skip: symlinkSkip }, () => {
  makeDir('home');
  makeDir('outside');
  fs.symlinkSync('home', 'in-home', 'dir');
  fs.symlinkSync('outside', 'out-of-home', 'dir');
  // Exercise the platform fallback in a child, without changing the test runner.
  const config = require.resolve('../hooks/puritanical-config');
  const script = [
    'Object.defineProperty(process, "getuid", { value: undefined });',
    'const { safeWriteFlag } = require(process.argv[1]);',
    'safeWriteFlag("in-home/.puritanical-active", "on");',
    'safeWriteFlag("out-of-home/.puritanical-active", "off");',
  ].join('\n');
  execFileSync(process.execPath, ['-e', script, config], {
    env: { ...process.env, HOME: absolutePath('home'), USERPROFILE: absolutePath('home') },
  });
  assert.equal(readText('home/.puritanical-active'), 'on');
  assert.equal(exists('outside/.puritanical-active'), false);
});

// ── getDefaultMode ───────────────────────────────────────────────────────────

test('getDefaultMode returns on by default', () => {
  const saved = process.env.PURITANICAL_DEFAULT_MODE;
  delete process.env.PURITANICAL_DEFAULT_MODE;
  // Only validates when no user config file exists (CI / clean env)
  const mode = getDefaultMode();
  assert.ok(VALID_MODES.includes(mode));
  if (saved !== undefined) process.env.PURITANICAL_DEFAULT_MODE = saved;
});

test('getDefaultMode respects PURITANICAL_DEFAULT_MODE env var', () => {
  process.env.PURITANICAL_DEFAULT_MODE = 'off';
  assert.equal(getDefaultMode(), 'off');
  process.env.PURITANICAL_DEFAULT_MODE = 'on';
  assert.equal(getDefaultMode(), 'on');
  delete process.env.PURITANICAL_DEFAULT_MODE;
});

test('getDefaultMode ignores invalid PURITANICAL_DEFAULT_MODE', () => {
  process.env.PURITANICAL_DEFAULT_MODE = 'wenyan-ultra';
  const mode = getDefaultMode();
  assert.ok(VALID_MODES.includes(mode));
  delete process.env.PURITANICAL_DEFAULT_MODE;
});

// ── major-version update notice ──────────────────────────────────────────────

const { majorOf, majorUpdateNotice, latestVersion, statuslineCommand, statuslineNotice } = require('../hooks/puritanical-activate');
const { isOwnedStatusLine } = require('../hooks/puritanical-config');
const path = require('path');

test('statuslineCommand matches the platform and produces a recognised statusline command', () => {
  const hooksDir = absolutePath('hooks-dir');
  const binDir = absolutePath('pwsh-bin');
  const emptyBin = absolutePath('empty-bin');
  makeDir('pwsh-bin');
  makeDir('empty-bin');

  const posixCmd = statuslineCommand(hooksDir, 'linux', '');
  assert.equal(posixCmd, `bash "${under(hooksDir, 'puritanical-statusline.sh')}"`);
  assert.equal(isOwnedStatusLine({ type: 'command', command: posixCmd }), true);

  const darwinCmd = statuslineCommand(hooksDir, 'darwin', '');
  assert.equal(darwinCmd, `bash "${under(hooksDir, 'puritanical-statusline.sh')}"`);

  const psCmd = statuslineCommand(hooksDir, 'win32', `${path.delimiter}${emptyBin}`);
  assert.equal(psCmd, `powershell -NoProfile -ExecutionPolicy Bypass -File "${under(hooksDir, 'puritanical-statusline.ps1')}"`);
  assert.equal(isOwnedStatusLine({ type: 'command', command: psCmd }), true);

  writeText(under(binDir, 'pwsh.exe'), '');
  const pwshCmd = statuslineCommand(hooksDir, 'win32', `${emptyBin}${path.delimiter}${binDir}`);
  assert.equal(pwshCmd, `pwsh -NoProfile -ExecutionPolicy Bypass -File "${under(hooksDir, 'puritanical-statusline.ps1')}"`);
  assert.equal(isOwnedStatusLine({ type: 'command', command: pwshCmd }), true);

  const missingSettings = under(absolutePath('hook'), 'missing-settings.json');
  const winNotice = statuslineNotice(missingSettings, 'win32', binDir);
  assert.match(winNotice, /STATUSLINE SETUP NEEDED:/);
  assert.ok(winNotice.includes('puritanical-statusline.ps1'));
  assert.ok(!winNotice.includes('puritanical-statusline.sh'));

  const posixNotice = statuslineNotice(missingSettings, 'linux', '');
  assert.match(posixNotice, /STATUSLINE SETUP NEEDED:/);
  assert.ok(posixNotice.includes('puritanical-statusline.sh'));
  assert.ok(!posixNotice.includes('puritanical-statusline.ps1'));
});

test('statuslineCommand picks pwsh when PATH holds a Store alias that stat refuses', (t) => {
  const fs = require('fs');
  const aliasDir = absolutePath('windows-apps');
  const realStat = fs.statSync;
  t.mock.method(fs, 'statSync', (p, ...rest) => {
    if (p === under(aliasDir, 'pwsh.exe')) throw Object.assign(new Error('EACCES: permission denied'), { code: 'EACCES' });
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Passes the stubbed call through to the real stat.
    return realStat(p, ...rest);
  });
  assert.match(statuslineCommand(absolutePath('hooks-dir'), 'win32', aliasDir), /^pwsh -NoProfile /);
  assert.match(statuslineCommand(absolutePath('hooks-dir'), 'win32', absolutePath('no-such-bin')), /^powershell -NoProfile /);
});

test('update lookup refreshes stale cache and reuses a fresh result offline', async (t) => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const cache = under(dir, 'update.json');
  safeWriteFlag(cache, JSON.stringify({ checkedAt: 0, latest: '1.0.0' }));
  const fetchMock = t.mock.method(globalThis, 'fetch', async () => ({
    ok: true, json: async () => ({ tag_name: 'v4.0.0' }),
  }));
  assert.equal(await latestVersion(cache), '4.0.0');
  assert.equal(readJsonFile(cache).latest, '4.0.0');
  assert.equal(await latestVersion(cache), '4.0.0');
  assert.equal(fetchMock.mock.callCount(), 1);
});

test('update lookup bounds the response body and preserves cache on failure', async (t) => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const cache = under(dir, 'update.json');
  const stale = { checkedAt: 0, latest: '1.0.0' };
  safeWriteFlag(cache, JSON.stringify(stale));
  const fetchMock = t.mock.method(globalThis, 'fetch', async () => ({ ok: false }));
  assert.equal(await latestVersion(cache), null);
  fetchMock.mock.mockImplementation(async (_url, { signal }) => ({
    ok: true,
    json: () => new Promise((resolve, reject) => {
      const timer = setTimeout(() => resolve({ tag_name: 'v99.0.0' }), 3000);
      signal.addEventListener('abort', () => {
        clearTimeout(timer);
        reject(signal.reason);
      }, { once: true });
    }),
  }));
  assert.equal(await latestVersion(cache), null);
  assert.deepEqual(readJsonFile(cache), stale);
});

test('majorOf parses majors, rejects garbage', () => {
  assert.equal(majorOf('1.1.2'), 1);
  assert.equal(majorOf('12.0.0'), 12);
  assert.equal(majorOf('nonsense'), null);
  assert.equal(majorOf(null), null);
});

test('notice fires only on a major jump', () => {
  assert.ok(majorUpdateNotice('1.1.2', '2.0.0'));
  assert.ok(majorUpdateNotice('1.9.9', '3.1.0'));
  assert.equal(majorUpdateNotice('1.1.2', '1.9.9'), null);  // minor: silent
  assert.equal(majorUpdateNotice('2.0.0', '2.0.1'), null);  // patch: silent
  assert.equal(majorUpdateNotice('2.0.0', '1.9.9'), null);  // downgrade: silent
  assert.equal(majorUpdateNotice(null, '2.0.0'), null);     // unknown install: silent
  assert.equal(majorUpdateNotice('1.0.0', null), null);     // no registry data: silent
  assert.equal(majorUpdateNotice('1.0.0', '4.0.0 extra text'), null);
  assert.match(majorUpdateNotice('1.0.0', '4.0.0-rc.1'), /4\.0\.0-rc\.1/);
});

test('requiring puritanical-activate has no side effects', () => {
  // If the require.main guard is broken this test file would have already
  // emitted the ruleset or called process.exit before reaching here.
  assert.ok(true);
});

// ── SessionStart injection gating ────────────────────────────────────────────
// Regression guard for #2 (@enc0ded): the full ruleset was re-injected on
// resume/clear/compact as well as startup, so the plugin's own overhead ate
// most of what the compressor saved.

const { execFileSync } = require('child_process');
const ACTIVATE = under(__dirname, '..', 'hooks', 'puritanical-activate.js');

function activate(stdin, extraEnv = {}) {
  const dir = absolutePath('hook');
  makeDir('hook');
  try {
    return execFileSync(process.execPath, [ACTIVATE], {
      input: stdin,
      env: { ...process.env, CLAUDE_CONFIG_DIR: dir, PURITANICAL_DEFAULT_MODE: 'on', PURITANICAL_UPDATE_CHECK: '0', ...extraEnv },
      encoding: 'utf8',
      timeout: 5000,
    });
  } finally {
    remove(dir, { recursive: true, force: true });
  }
}

test('REGRESSION #2: startup gets the full ruleset', () => {
  const out = activate(JSON.stringify({ source: 'startup' }));
  assert.ok(out.length > 2000, `expected full ruleset, got ${out.length} chars`);
  assert.match(out, /PURITANICAL ACTIVE/);
});

test('startup honours disabled mode and explains a legacy intensity setting', () => {
  assert.equal(activate('{}', { PURITANICAL_DEFAULT_MODE: 'off' }), 'OK');
  assert.match(activate('{}', { PURITANICAL_DEFAULT_MODE: 'lite' }), /PURITANICAL NOTE: .*lite.*PURITANICAL_DEFAULT_MODE/);
});

test('REGRESSION #2: resume/clear/compact do not re-inject the ruleset', () => {
  for (const source of ['resume', 'clear', 'compact']) {
    const out = activate(JSON.stringify({ source }));
    assert.ok(out.length < 400, `${source} re-injected ${out.length} chars`);
    // still has to announce the mode, or the session silently loses it
    assert.match(out, /PURITANICAL ACTIVE/);
  }
});

test('unknown or malformed source falls back to a full inject', () => {
  // failing open (over-teaching once) beats a session with no ruleset at all
  assert.ok(activate('not json').length > 2000);
  assert.ok(activate('').length > 2000);
  assert.ok(activate(JSON.stringify({})).length > 2000);
});

// ── Version sync across manifests ────────────────────────────────────────────
// The update notice reads .claude-plugin/plugin.json, not package.json. When
// those drifted (plugin 1.2.2 vs published 2.0.0) every user on the current
// release was told a major update was available. Keep them equal.

test('every manifest carries the same version', () => {
  const versions = [
    ['package.json', require('../package.json').version],
    ['.claude-plugin/plugin.json', require('../.claude-plugin/plugin.json').version],
    ['.github/plugin/plugin.json', require('../.github/plugin/plugin.json').version],
    ['.codex-plugin/plugin.json', require('../.codex-plugin/plugin.json').version],
    ['gemini-extension.json', require('../gemini-extension.json').version],
  ];
  const expected = versions[0][1];
  for (const [f, v] of versions) {
    assert.equal(v, expected, `${f} is ${v}, expected ${expected}`);
  }
});

test('the changelog documents the shipped version', () => {
  const root = under(__dirname, '..');
  const v = readJson(under(root, 'package.json')).version;
  const log = readText(under(root, 'CHANGELOG.md'));
  assert.ok(log.includes(`## [${v}]`), `CHANGELOG.md has no section for ${v}`);
});

// ── 3.0.0 upgrade path ───────────────────────────────────────────────────────
// A lite/full/ultra setting from before 3.0.0 must not break anything, and must
// not be ignored in silence either.

const { legacySetting } = require('../hooks/puritanical-config');

test('a legacy level setting still resolves to on, not off', () => {
  for (const stale of ['lite', 'full', 'ultra']) {
    process.env.PURITANICAL_DEFAULT_MODE = stale;
    assert.equal(getDefaultMode(), 'on', `${stale} should fall through to on`);
  }
  delete process.env.PURITANICAL_DEFAULT_MODE;
});

test('a legacy level setting is detected so it can be reported', () => {
  process.env.PURITANICAL_DEFAULT_MODE = 'ultra';
  const l = legacySetting();
  assert.equal(l.value, 'ultra');
  assert.equal(l.source, 'PURITANICAL_DEFAULT_MODE');
  delete process.env.PURITANICAL_DEFAULT_MODE;
});

test('a current setting reports no legacy value', () => {
  process.env.PURITANICAL_DEFAULT_MODE = 'on';
  assert.equal(legacySetting(), null);
  delete process.env.PURITANICAL_DEFAULT_MODE;
});

// ── sections config: getSections / filterSections ────────────────────────────
// `{ "sections": { "prose": false } }` in config.json suppresses prose rules,
// keeping code rules (and vice versa). Config-file only, no env override.
// Absent/malformed/non-boolean always falls back to enabled; never throws.

const { getSections, filterSections, PROSE_HEADINGS, CODE_HEADINGS } = require('../hooks/puritanical-config');

function withSectionsConfig(sectionsValue, fn) {
  const dir = absolutePath('hook');
  makeDir('hook');
  const puritanicalDir = under(dir, 'puritanical');
  makeDir(puritanicalDir, { recursive: true });
  if (sectionsValue !== undefined) {
    writeText(under(puritanicalDir, 'config.json'), JSON.stringify({ defaultMode: 'on', sections: sectionsValue }));
  }
  const saved = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = dir;
  try {
    return fn();
  } finally {
    if (saved === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = saved;
    remove(dir, { recursive: true, force: true });
  }
}

test('getSections: no config file → both enabled', () => {
  withSectionsConfig(undefined, () => {
    assert.deepEqual(getSections(), { prose: true, code: true });
  });
});

test('getSections: sections.prose false → prose disabled, code enabled', () => {
  withSectionsConfig({ prose: false }, () => {
    assert.deepEqual(getSections(), { prose: false, code: true });
  });
});

test('getSections: sections.code false → code disabled, prose enabled', () => {
  withSectionsConfig({ code: false }, () => {
    assert.deepEqual(getSections(), { prose: true, code: false });
  });
});

test('getSections: both false', () => {
  withSectionsConfig({ prose: false, code: false }, () => {
    assert.deepEqual(getSections(), { prose: false, code: false });
  });
});

test('getSections: malformed sections values fall back to enabled, never throw', () => {
  for (const malformed of ['off', null, 5, [], { prose: 'no' }, { prose: null }, { prose: 1 }, { prose: [] }]) {
    withSectionsConfig(malformed, () => {
      assert.deepEqual(getSections(), { prose: true, code: true }, JSON.stringify(malformed));
    });
  }
});

test('getSections: malformed config.json (invalid JSON) falls back to enabled, never throws', () => {
  const dir = absolutePath('hook');
  makeDir('hook');
  const puritanicalDir = under(dir, 'puritanical');
  makeDir(puritanicalDir, { recursive: true });
  writeText(under(puritanicalDir, 'config.json'), '{ not json');
  const saved = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = dir;
  try {
    assert.deepEqual(getSections(), { prose: true, code: true });
  } finally {
    if (saved === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = saved;
    remove(dir, { recursive: true, force: true });
  }
});

const SKILL_BODY = readText(under(__dirname, '..', 'skills', 'puritanical', 'SKILL.md')).replace(/^---[\s\S]*?---\s*/, '');
const ALWAYS_ON_HEADINGS = [
  'Persistence', 'Thinking Is Billed Too', 'Auto-Clarity', 'When NOT to be lazy', 'Boundaries',
];

test('filterSections: both enabled is byte-identical to input', () => {
  assert.equal(filterSections(SKILL_BODY, { prose: true, code: true }), SKILL_BODY);
});

test('filterSections: prose false drops only the prose headings', () => {
  const out = filterSections(SKILL_BODY, { prose: false, code: true });
  for (const h of PROSE_HEADINGS) assert.ok(!out.includes(`## ${h}`), `still has ${h}`);
  for (const h of CODE_HEADINGS) assert.ok(out.includes(`## ${h}`), `missing ${h}`);
  for (const h of ALWAYS_ON_HEADINGS) assert.ok(out.includes(`## ${h}`), `missing ${h}`);
});

test('filterSections: code false drops only the code headings', () => {
  const out = filterSections(SKILL_BODY, { prose: true, code: false });
  for (const h of CODE_HEADINGS) assert.ok(!out.includes(`## ${h}`), `still has ${h}`);
  for (const h of PROSE_HEADINGS) assert.ok(out.includes(`## ${h}`), `missing ${h}`);
  for (const h of ALWAYS_ON_HEADINGS) assert.ok(out.includes(`## ${h}`), `missing ${h}`);
});

test('filterSections: both false still emits every always-on heading', () => {
  const out = filterSections(SKILL_BODY, { prose: false, code: false });
  for (const h of [...PROSE_HEADINGS, ...CODE_HEADINGS]) assert.ok(!out.includes(`## ${h}`), `still has ${h}`);
  for (const h of ALWAYS_ON_HEADINGS) assert.ok(out.includes(`## ${h}`), `missing ${h}`);
});

test('filterSections: malformed sections values fall back to enabled (identity)', () => {
  for (const malformed of ['off', null, 5, [], undefined]) {
    assert.equal(filterSections(SKILL_BODY, malformed), SKILL_BODY);
  }
});

// ── SessionStart end-to-end: sections config wired into the emitted ruleset ──

test('activate: no config file → full ruleset, all headings present (default unchanged)', () => {
  const dir = absolutePath('xdg');
  makeDir('xdg');
  try {
    const out = activate(JSON.stringify({ source: 'startup' }), { XDG_CONFIG_HOME: dir });
    for (const h of [...PROSE_HEADINGS, ...CODE_HEADINGS, ...ALWAYS_ON_HEADINGS]) {
      assert.ok(out.includes(`## ${h}`), `missing ${h}`);
    }
  } finally {
    remove(dir, { recursive: true, force: true });
  }
});

test('activate: config file present but no sections key → full ruleset (default unchanged)', () => {
  const dir = absolutePath('xdg');
  makeDir('xdg');
  const puritanicalDir = under(dir, 'puritanical');
  makeDir(puritanicalDir, { recursive: true });
  writeText(under(puritanicalDir, 'config.json'), JSON.stringify({ defaultMode: 'on' }));
  try {
    const out = activate(JSON.stringify({ source: 'startup' }), { XDG_CONFIG_HOME: dir });
    for (const h of [...PROSE_HEADINGS, ...CODE_HEADINGS, ...ALWAYS_ON_HEADINGS]) {
      assert.ok(out.includes(`## ${h}`), `missing ${h}`);
    }
  } finally {
    remove(dir, { recursive: true, force: true });
  }
});

test('activate: sections.prose false → emitted ruleset drops prose headings only', () => {
  const dir = absolutePath('xdg');
  makeDir('xdg');
  const puritanicalDir = under(dir, 'puritanical');
  makeDir(puritanicalDir, { recursive: true });
  writeText(under(puritanicalDir, 'config.json'), JSON.stringify({ sections: { prose: false } }));
  try {
    const out = activate(JSON.stringify({ source: 'startup' }), { XDG_CONFIG_HOME: dir });
    for (const h of PROSE_HEADINGS) assert.ok(!out.includes(`## ${h}`), `still has ${h}`);
    for (const h of [...CODE_HEADINGS, ...ALWAYS_ON_HEADINGS]) assert.ok(out.includes(`## ${h}`), `missing ${h}`);
  } finally {
    remove(dir, { recursive: true, force: true });
  }
});

test('activate: malformed sections value falls back to enabled, does not throw', () => {
  const dir = absolutePath('xdg');
  makeDir('xdg');
  const puritanicalDir = under(dir, 'puritanical');
  makeDir(puritanicalDir, { recursive: true });
  writeText(under(puritanicalDir, 'config.json'), JSON.stringify({ sections: 'nope' }));
  try {
    const out = activate(JSON.stringify({ source: 'startup' }), { XDG_CONFIG_HOME: dir });
    for (const h of [...PROSE_HEADINGS, ...CODE_HEADINGS, ...ALWAYS_ON_HEADINGS]) {
      assert.ok(out.includes(`## ${h}`), `missing ${h}`);
    }
  } finally {
    remove(dir, { recursive: true, force: true });
  }
});

// The section filter matches SKILL.md headings by exact string, from constants
// living in hooks/. That is the one place this repo duplicates SKILL.md content
// into code, against its own "SKILL.md is the source of truth" rule, so it needs
// a guard: rename a heading in SKILL.md and the filter silently stops matching
// it — the section just keeps shipping, with no error and no failing assertion,
// because every other test checks "is it absent" against the same stale string.
test('every PROSE/CODE heading constant still exists in SKILL.md', () => {
  const md = readText(under(__dirname, '..', 'skills', 'puritanical', 'SKILL.md'));
  const present = [...md.matchAll(/^## (.+)$/gm)].map((m) => m[1].trim());
  for (const h of [...PROSE_HEADINGS, ...CODE_HEADINGS]) {
    assert.ok(present.includes(h),
      `heading constant "${h}" is not in SKILL.md — filterSections would silently skip it`);
  }
});

// The inverse: a heading added to SKILL.md that belongs to neither group, and is
// not one of the deliberate always-on sections, is almost certainly an oversight
// — it would ship even with both groups suppressed.
test('every SKILL.md heading is classified: prose, code, or deliberately always-on', () => {
  const ALWAYS_ON = [
    'Persistence', 'Thinking Is Billed Too', 'Auto-Clarity',
    'When NOT to be lazy', 'Boundaries',
  ];
  const md = readText(under(__dirname, '..', 'skills', 'puritanical', 'SKILL.md'));
  const classified = new Set([...PROSE_HEADINGS, ...CODE_HEADINGS, ...ALWAYS_ON]);
  for (const m of md.matchAll(/^## (.+)$/gm)) {
    const h = m[1].trim();
    assert.ok(classified.has(h),
      `SKILL.md heading "${h}" is in no group — add it to PROSE_HEADINGS, CODE_HEADINGS, or ALWAYS_ON`);
  }
});

// filterSections splits on /^## /m, so a "## " line inside a fenced code block
// would cut the fence in half and emit broken markdown into the model's context.
test('no SKILL.md heading-like line hides inside a fenced code block', () => {
  const md = readText(under(__dirname, '..', 'skills', 'puritanical', 'SKILL.md'));
  let inFence = false;
  md.split('\n').forEach((line, i) => {
    if (line.trim().startsWith('```')) { inFence = !inFence; return; }
    assert.ok(!(inFence && line.startsWith('## ')),
      `SKILL.md:${i + 1} has a "## " line inside a code fence; filterSections would split it`);
  });
  assert.equal(inFence, false, 'SKILL.md has an unclosed code fence');
});

// ── Automatic output-style detection (#17) ──────────────────────────────────
// Claude Code's output styles govern prose structure, so an active style and
// Puritanical's prose section contradict each other (#15). `outputStyle` is a
// documented settings key and `/output-style` persists it to
// .claude/settings.local.json, so the active style is readable even though the
// SessionStart payload does not carry it.
const { getActiveOutputStyle, resolveSections } = require('../hooks/puritanical-config');

function settingsText(value) {
  return typeof value === 'string' ? value : JSON.stringify(value);
}

function styleFixture() {
  const root = absolutePath('style');
  makeDir('style');
  makeDir(under(root, 'proj', '.claude'), { recursive: true });
  makeDir(under(root, 'home', '.claude'), { recursive: true });
  makeDir(under(root, 'cfg', 'puritanical'), { recursive: true });
  return {
    root,
    cwd: under(root, 'proj'),
    claudeDir: under(root, 'home', '.claude'),
    cfg: under(root, 'cfg'),
  };
}

test('getActiveOutputStyle honours Claude Code precedence: local > project > user', () => {
  const f = styleFixture();
  try {
    const at = () => getActiveOutputStyle({ cwd: f.cwd, claudeDir: f.claudeDir });
    assert.equal(at(), null, 'nothing set');
    writeText('style/home/.claude/settings.json', settingsText({ outputStyle: 'Explanatory' }));
    assert.equal(at(), 'Explanatory');
    writeText('style/proj/.claude/settings.json', settingsText({ outputStyle: 'Learning' }));
    assert.equal(at(), 'Learning', 'project must beat user');
    writeText('style/proj/.claude/settings.local.json', settingsText({ outputStyle: 'concise' }));
    assert.equal(at(), 'concise', 'local must beat project — /output-style writes here');
  } finally { remove(f.root, { recursive: true, force: true }); }
});

test('getActiveOutputStyle treats the built-in "default" as no style', () => {
  const f = styleFixture();
  try {
    for (const v of ['default', 'Default', 'DEFAULT']) {
      writeText('style/proj/.claude/settings.local.json', settingsText({ outputStyle: v }));
      assert.equal(getActiveOutputStyle({ cwd: f.cwd, claudeDir: f.claudeDir }), null, v);
    }
  } finally { remove(f.root, { recursive: true, force: true }); }
});

test('getActiveOutputStyle falls through junk without throwing', () => {
  const f = styleFixture();
  try {
    const at = () => getActiveOutputStyle({ cwd: f.cwd, claudeDir: f.claudeDir });
    writeText('style/home/.claude/settings.json', settingsText({ outputStyle: 'Explanatory' }));
    for (const junk of ['{ not json', JSON.stringify({ outputStyle: '   ' }),
                        JSON.stringify({ outputStyle: 42 }), JSON.stringify({ outputStyle: null }),
                        JSON.stringify([]), '']) {
      writeText('style/proj/.claude/settings.local.json', settingsText(junk));
      assert.equal(at(), 'Explanatory', `should fall through: ${junk.slice(0, 24)}`);
    }
    // JSONC: Claude parses strict JSON but users leave comments in settings.
    writeText('style/proj/.claude/settings.local.json', settingsText('{ /* mine */ "outputStyle": "Proactive" } '));
    assert.equal(at(), 'Proactive');
  } finally { remove(f.root, { recursive: true, force: true }); }
});

test('resolveSections: an active style suppresses prose but never code', () => {
  const f = styleFixture();
  const saved = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = f.cfg;
  try {
    const r = () => resolveSections({ cwd: f.cwd, claudeDir: f.claudeDir });
    assert.deepEqual(r(), { prose: true, code: true, outputStyle: null }, 'no style = untouched');
    writeText('style/proj/.claude/settings.local.json', settingsText({ outputStyle: 'Explanatory' }));
    assert.deepEqual(r(), { prose: false, code: true, outputStyle: 'Explanatory' });
  } finally {
    if (saved === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = saved;
    remove(f.root, { recursive: true, force: true });
  }
});

test('resolveSections: an explicit sections.prose always beats detection', () => {
  const f = styleFixture();
  const saved = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = f.cfg;
  try {
    writeText('style/proj/.claude/settings.local.json', settingsText({ outputStyle: 'Explanatory' }));
    const r = () => resolveSections({ cwd: f.cwd, claudeDir: f.claudeDir });

    // The important one: a user who asked for prose rules keeps them.
    writeText('style/cfg/puritanical/config.json', settingsText({ sections: { prose: true } }));
    assert.equal(r().prose, true, 'explicit true must survive an active style');

    writeText('style/cfg/puritanical/config.json', settingsText({ sections: { prose: false } }));
    assert.equal(r().prose, false);

    // Only `code` set: prose is still unset, so detection applies.
    writeText('style/cfg/puritanical/config.json', settingsText({ sections: { code: false } }));
    assert.deepEqual(r(), { prose: false, code: false, outputStyle: 'Explanatory' });

    // Non-boolean is not "explicit" — it falls back and detection applies.
    writeText('style/cfg/puritanical/config.json', settingsText({ sections: { prose: 'yes' } }));
    assert.equal(r().prose, false);
  } finally {
    if (saved === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = saved;
    remove(f.root, { recursive: true, force: true });
  }
});
