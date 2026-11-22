#!/usr/bin/env node
/* global __dirname, process, require -- Node.js runtime */
// Integration tests for bin/install.js — runs the real CLI against temp dirs.
// Covers the no-shell-out paths: project rule install, dry-run, list, idempotency.
// Run: node --test tests/test_installer.js

const { test } = require('node:test');
const assert = require('node:assert/strict');
const {
  absolutePath, exists, listDir, makeDir, readJson, readText, under, writeText,
} = require('./helpers/sandbox');
const fs = require('fs');
const isolateWorkingDirectory = require('./fixtures/working-directory');
const path = require('path');
const { execFileSync } = require('child_process');
const { readSettings } = require('../bin/lib/settings');
const { readFlag } = require('../hooks/chisle-config');
const { recordSavings } = require('../hooks/chisle-compress-output');

const CLI = under(__dirname, '..', 'bin', 'install.js');

isolateWorkingDirectory();

function runCLI(args, { cwd, env } = {}) {
  try {
    const out = execFileSync(process.execPath, [CLI, ...args, '--no-color'], {
      cwd: cwd || process.cwd(), encoding: 'utf8', timeout: 10000,
      env: env ? { ...process.env, ...env } : process.env,
    });
    return { status: 0, out };
  } catch (e) {
    return { status: e.status ?? 1, out: (e.stdout || '') + (e.stderr || '') };
  }
}

test('--help lists all agents', () => {
  const { out } = runCLI(['--help']);
  for (const id of ['claude', 'pi', 'gemini', 'codex', 'cursor', 'windsurf', 'cline', 'kiro', 'copilot', 'antigravity']) {
    assert.ok(out.includes(id), id);
  }
});

test('--list shows detection marks without installing', () => {
  const { out } = runCLI(['--list']);
  assert.match(out, /Claude Code/);
  assert.match(out, /project-scoped/);
});

test('unknown flag exits 2', () => {
  const { status, out } = runCLI(['--frobnicate']);
  assert.equal(status, 2);
  assert.match(out, /unknown flag/);
});

test('--only with bad agent exits 2', () => {
  const { status } = runCLI(['--only', 'notanagent']);
  assert.equal(status, 2);
});

test('project rule install writes into CWD, is idempotent, --force overwrites', () => {
  const proj = absolutePath('project');
  makeDir('project');

  // First install
  let r = runCLI(['--only', 'cursor'], { cwd: proj });
  const ruleFile = under(proj, '.cursor', 'rules', 'chisle.mdc');
  assert.ok(exists(ruleFile), 'rule file created in project');
  assert.match(r.out, /installed:/);

  // Second install — idempotent skip
  r = runCLI(['--only', 'cursor'], { cwd: proj });
  assert.match(r.out, /already in this project/);

  // Tamper, then --force restores
  writeText(ruleFile, 'TAMPERED');
  r = runCLI(['--only', 'cursor', '--force'], { cwd: proj });
  assert.notEqual(readText(ruleFile), 'TAMPERED');
});

test('Antigravity rule lands in .agents/rules as an always-on workspace rule', () => {
  const proj = absolutePath('project');
  makeDir('project');
  const r = runCLI(['--only', 'antigravity'], { cwd: proj });
  const ruleFile = under(proj, '.agents', 'rules', 'chisle.md');
  assert.match(r.out, /installed:/);
  // Antigravity ignores a workspace rule without a `trigger` in its frontmatter.
  assert.match(readText(ruleFile), /^---\ntrigger: always_on\n/);
});

test('Pi package manifest ships extension and skill', () => {
  const pkg = require('../package.json');
  assert.deepEqual(pkg.pi.extensions, ['./pi-extension/index.js']);
  assert.deepEqual(pkg.pi.skills, ['./skills']);
  assert.ok(pkg.files.includes('pi-extension/'));
  assert.ok(exists(under(__dirname, '..', 'pi-extension', 'index.js')));
});

test('OMP manifest points at a .cjs entry resolving to the same factory', () => {
  // OMP classifies a CommonJS entry named .js as ESM, so module.exports never
  // reaches the default export and its factory lookup fails. Only the entry
  // filename matters: a required .js dependency is classified correctly.
  // Resolution is `pkg.omp ?? pkg.pi`, so Pi keeps loading index.js and
  // nothing double-loads.
  const pkg = require('../package.json');
  assert.deepEqual(pkg.omp.extensions, ['./pi-extension/index.cjs']);
  assert.deepEqual(pkg.omp.skills, ['./skills']);
  assert.ok(pkg.files.includes('pi-extension/'));
  assert.ok(exists(under(__dirname, '..', 'pi-extension', 'index.cjs')));
  assert.equal(require('../pi-extension/index.cjs'), require('../pi-extension/index.js'));
});

test('dry-run changes nothing on disk', () => {
  const proj = absolutePath('project');
  makeDir('project');
  runCLI(['--only', 'cline', '--dry-run'], { cwd: proj });
  assert.ok(!exists(under(proj, '.clinerules')), 'dry-run wrote nothing');
});

test('Pi install, dry-run, and uninstall delegate settings changes to Pi', { skip: process.platform === 'win32' }, () => {
  const home = absolutePath('home');
  makeDir('home');
  const binDir = absolutePath('bin');
  makeDir('bin');
  const log = under(home, 'pi.log');
  const list = under(home, 'pi-list.txt');
  const settings = under(home, '.pi', 'agent', 'settings.json');
  makeDir(path.dirname(settings), { recursive: true });
  writeText(settings, JSON.stringify({ theme: 'dark', packages: ['npm:foreign'] }));
  const fake = under(binDir, 'pi');
  writeText(fake, '#!/bin/sh\necho "$@" >> "$PI_LOG"\n[ "$1" = list ] && cat "$PI_LIST"\nexit 0\n', { mode: 0o755 });
  const env = {
    HOME: home,
    PATH: binDir + path.delimiter + process.env.PATH,
    PI_LOG: log,
    PI_LIST: list,
  };

  let result = runCLI(['--only', 'pi'], { env });
  assert.match(result.out, /Pi detected/);
  assert.match(readText(log), /^list\ninstall git:github\.com\/yungSNAT\/Puritanical\n$/);
  assert.deepEqual(readJson(settings), { theme: 'dark', packages: ['npm:foreign'] },
    'installer edited Pi settings instead of delegating to Pi');

  writeText(list, 'User packages:\n  git:github.com/yungSNAT/Puritanical@v4 (user)\n');
  writeText(log, '');
  result = runCLI(['--only', 'pi'], { env });
  assert.match(result.out, /already installed/);
  assert.equal(readText(log), 'list\n');

  // An upstream package from an earlier install is replaced, not skipped.
  writeText(list, 'User packages:\n  npm:chisle@3.0.0\n');
  writeText(log, '');
  result = runCLI(['--only', 'pi'], { env });
  assert.doesNotMatch(result.out, /already installed/);
  assert.equal(readText(log), 'list\nremove npm:chisle\ninstall git:github.com/yungSNAT/Puritanical\n');

  writeText(list, '');
  writeText(log, '');
  result = runCLI(['--only', 'pi', '--dry-run'], { env });
  assert.match(result.out, /would run: pi install git:github\.com\/yungSNAT\/Puritanical/);
  assert.equal(readText(log), 'list\n', 'dry-run invoked mutating Pi command');

  writeText(log, '');
  writeText(list, 'User packages:\n  npm:chisle@3.0.0\n');
  result = runCLI(['--uninstall', '--only', 'pi'], { env });
  assert.match(result.out, /Uninstalled/);
  assert.match(readText(log), /^list\nremove npm:chisle\n$/);
  assert.deepEqual(readJson(settings), { theme: 'dark', packages: ['npm:foreign'] },
    'uninstaller edited unrelated Pi settings');
});

test('standalone Claude hook wiring merges + uninstalls cleanly', () => {
  // Drive the standalone path by pointing --config-dir at a temp dir and using
  // --only claude. Uninstall must stay scoped (#39): without --only it removes
  // every host's integration, and with a real `claude` on PATH it would run
  // `claude plugin uninstall` against the live install. So: empty PATH, temp HOME.
  const cfg = absolutePath('Claude config');
  makeDir('Claude config');
  const home = absolutePath('home');
  makeDir('home');
  const emptyBin = absolutePath('empty-bin');
  makeDir('empty-bin');
  const env = { HOME: home, USERPROFILE: home, PATH: emptyBin };
  // Pre-seed a user settings.json with foreign content to protect.
  writeText(under(cfg, 'settings.json'), JSON.stringify({ model: 'opus' }, null, 2));

  runCLI(['--only', 'claude', '--config-dir', cfg, '--dry-run'], { env });
  // Dry-run must not have altered the seeded file.
  assert.deepEqual(readJson(under(cfg, 'settings.json')), { model: 'opus' });

  const install = ['--only', 'claude', '--config-dir', cfg];
  assert.equal(runCLI(install, { env }).status, 0);
  const settingsPath = under(cfg, 'settings.json');
  const installed = readSettings(settingsPath);
  const hookPath = under(cfg, 'chisle-hooks', 'chisle-activate.js');
  assert.equal(installed.hooks.SessionStart[0].hooks[0].command, `"${process.execPath}" "${hookPath}"`);
  const extension = process.platform === 'win32' ? 'ps1' : 'sh';
  assert.ok(installed.statusLine.command.endsWith(`"${under(cfg, 'chisle-hooks', 'chisle-statusline.' + extension)}"`));
  assert.equal(runCLI([...install, '--update'], { env }).status, 0);
  assert.deepEqual(readSettings(settingsPath), installed);
  const startup = execFileSync(process.execPath, [hookPath], {
    input: '{}', encoding: 'utf8', timeout: 5000,
    env: { ...process.env, ...env, CLAUDE_CONFIG_DIR: cfg, XDG_CONFIG_HOME: home, CHISLE_DEFAULT_MODE: 'on' },
  });
  assert.match(startup, /CHISLE ACTIVE/);
  assert.doesNotMatch(startup, /STATUSLINE SETUP NEEDED/);
  assert.equal(readFlag(under(cfg, '.chisle-active')), 'on');
  if (process.platform === 'win32') {
    recordSavings(4000, cfg);
    const powershell = under(process.env.SystemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
    const badge = execFileSync(powershell, ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
      under(cfg, 'chisle-hooks', 'chisle-statusline.ps1')], {
      encoding: 'utf8', timeout: 10000, env: { ...process.env, CLAUDE_CONFIG_DIR: cfg },
    });
    assert.match(badge, /\[CHISLE\].*1k tok/);
    assert.doesNotMatch(badge, /[\r\n]/);
  }

  // Uninstall against the same dir must not corrupt the foreign settings.
  runCLI(['--uninstall', '--only', 'claude', '--config-dir', cfg], { env });
  const after = readJson(under(cfg, 'settings.json'));
  assert.equal(after.model, 'opus', 'foreign settings preserved through uninstall');
  assert.equal(after.statusLine, undefined);
  assert.equal(after.hooks, undefined);
  assert.equal(exists('Claude config/chisle-hooks/chisle-activate.js'), false);
});

test('star prompt: never asked, never written, never calls gh without a terminal', { skip: process.platform === 'win32' }, () => {
  const cfg = absolutePath('config');
  makeDir('config');
  const home = absolutePath('home');
  makeDir('home');
  const bin = absolutePath('bin');
  makeDir('bin');
  const log = under(bin, 'gh.log');
  writeText(under(bin, 'gh'), `#!/bin/sh\necho "$*" >> "${log}"\n`, { mode: 0o755 });
  const env = { HOME: home, USERPROFILE: home, XDG_CONFIG_HOME: under(home, 'xdg'), PATH: bin, CI: '' };
  const { status, out } = runCLI(['--only', 'claude', '--config-dir', cfg], { env });
  assert.equal(status, 0);
  assert.doesNotMatch(out, /\[l\]ater/);
  assert.match(out, /a star helps others find it: https:\/\/github\.com\/yungSNAT\/Puritanical/);
  assert.equal(exists(log), false, 'gh never called');
  assert.equal(exists(under(home, 'xdg', 'chisle', 'config.json')), false, 'nothing saved');
});

// Regression for #4: the plugin path and the standalone path both registering
// the hooks made two copies run per tool call, and the shared dedup state file
// then reported first-seen output as a duplicate of itself.
test('REGRESSION: plugin path removes leftover standalone hook entries', { skip: process.platform === 'win32' }, () => {
  const cfg = absolutePath('config');
  makeDir('config');
  const binDir = absolutePath('bin');
  makeDir('bin');
  // A `claude` that reports the plugin as installed, so installClaude() takes
  // the plugin branch without shelling out to a real marketplace.
  const fake = under(binDir, 'claude');
  writeText(fake, '#!/bin/sh\necho chisle\nexit 0\n', { mode: 0o755 });

  // Seed the state an affected machine is in: standalone wiring plus a foreign
  // hook that must survive.
  writeText(under(cfg, 'settings.json'), JSON.stringify({
    model: 'opus',
    hooks: {
      PostToolUse: [
        { hooks: [{ type: 'command', command: 'node ~/.claude/chisle-hooks/chisle-compress-output.js' }] },
        { hooks: [{ type: 'command', command: 'node ~/.claude/other/keep-me.js' }] },
      ],
    },
  }, null, 2));

  const env = { PATH: binDir + path.delimiter + process.env.PATH };
  runCLI(['--only', 'claude', '--config-dir', cfg], { env });

  const after = readJson(under(cfg, 'settings.json'));
  const cmds = ((after.hooks && after.hooks.PostToolUse) || [])
    .flatMap(e => (e.hooks || []).map(h => h.command));
  assert.ok(!cmds.some(c => c.includes('chisle-')), 'standalone chisle hook still registered');
  assert.ok(cmds.some(c => c.includes('keep-me.js')), 'foreign hook removed');
  assert.equal(after.model, 'opus');
});

// ── --stats ─────────────────────────────────────────────────────────────────
// The ledger is written by the compressor hook, so these tests seed it directly
// and assert the CLI reports what is actually there, never an estimate.

test('--stats reports the ledger and changes nothing', () => {
  const cfg = absolutePath('config');
  makeDir('config');
  const ledger = under(cfg, '.chisle-compress-stats.json');
  writeText(ledger, JSON.stringify({ savedChars: 88967, events: 18 }));
  const before = readText(ledger);

  const r = runCLI(['--stats', '--config-dir', cfg]);
  assert.equal(r.status, 0);
  assert.match(r.out, /88,967 chars/);
  assert.match(r.out, /22,241 tokens/);   // chars / 4, floored
  assert.match(r.out, /18 compressed/);
  assert.equal(readText(ledger), before, '--stats must be read-only');
  assert.equal(listDir(cfg).length, 1, '--stats must not create files');
});

test('--stats says so plainly when nothing has been recorded', () => {
  const cfg = absolutePath('config');
  makeDir('config');
  const r = runCLI(['--stats', '--config-dir', cfg]);
  assert.equal(r.status, 0);
  assert.match(r.out, /no compression recorded yet/);
  assert.doesNotMatch(r.out, /NaN|undefined/);
});

test('--stats survives a corrupt or hostile ledger', () => {
  const cfg = absolutePath('config');
  makeDir('config');
  for (const junk of ['{', 'null', '{"savedChars":"lots","events":3}', '[]']) {
    writeText(under(cfg, '.chisle-compress-stats.json'), junk);
    const r = runCLI(['--stats', '--config-dir', cfg]);
    assert.equal(r.status, 0, `crashed on ${junk}`);
    assert.match(r.out, /no compression recorded yet/, `trusted ${junk}`);
  }
});

test('--stats refuses to follow a symlinked ledger', { skip: process.platform === 'win32' }, () => {
  const cfg = absolutePath('config');
  makeDir('config');
  const real = under(cfg, 'elsewhere.json');
  writeText(real, JSON.stringify({ savedChars: 99999, events: 7 }));
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
  fs.symlinkSync(real, under(cfg, '.chisle-compress-stats.json')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

  const r = runCLI(['--stats', '--config-dir', cfg]);
  assert.equal(r.status, 0);
  assert.doesNotMatch(r.out, /99,999/, 'followed a symlink');
});

test('--help and --list advertise --stats', () => {
  assert.match(runCLI(['--help']).out, /--stats/);
});

// ── --update ────────────────────────────────────────────────────────────────
// The bug this flag exists for: `npx chisle` skips whatever is already there,
// so the documented upgrade command reported success and changed nothing.

function piUpdateFixture(installed) {
  const home = absolutePath('home');
  makeDir('home');
  const binDir = absolutePath('bin');
  makeDir('bin');
  const log = under(home, 'pi.log');
  const list = under(home, 'pi-list.txt');
  writeText('home/pi-list.txt', 'User packages:\n  ' + installed + '\n');
  writeText(under(binDir, 'pi'), '#!/bin/sh\necho "$@" >> "$PI_LOG"\n[ "$1" = list ] && cat "$PI_LIST"\nexit 0\n', { mode: 0o755 });
  return { HOME: home, PATH: binDir + path.delimiter + process.env.PATH, PI_LOG: log, PI_LIST: list };
}

test('--update refreshes an installed Pi package instead of skipping it', { skip: process.platform === 'win32' }, () => {
  const env = piUpdateFixture('git:github.com/yungSNAT/Puritanical@v4 (user)');

  // plain install: sees it, leaves it alone
  const plain = runCLI(['--only', 'pi'], { env });
  assert.match(plain.out, /already installed/);

  // update: reinstalls the same agent
  writeText('home/pi.log', '');
  const upd = runCLI(['--only', 'pi', '--update'], { env });
  assert.match(upd.out, /Chisle updater/);
  assert.doesNotMatch(upd.out, /already installed/);
  assert.match(readText('home/pi.log'), /install git:github\.com\/yungSNAT\/Puritanical/, '--update did not reinstall');
});

test('--update installs nothing new and says why', { skip: process.platform === 'win32' }, () => {
  const env = piUpdateFixture('npm:something-else@1.0.0');

  const r = runCLI(['--only', 'pi', '--update'], { env });
  assert.match(r.out, /Nothing to update/);
  assert.doesNotMatch(readText('home/pi.log'), /install/, '--update installed an absent agent');
});

test('--update refreshes a project rule file in place', () => {
  const proj = absolutePath('project');
  makeDir('project');
  const rule = under(proj, '.clinerules', 'chisle.md');

  runCLI(['--only', 'cline'], { cwd: proj });
  assert.ok(exists(rule));
  writeText(rule, 'stale copy from an older release\n');

  const r = runCLI(['--only', 'cline', '--update'], { cwd: proj });
  assert.match(r.out, /Chisle updater/);
  assert.doesNotMatch(readText(rule), /stale copy/, '--update left a stale rule file');
});

test('--update is read-only under --dry-run', { skip: process.platform === 'win32' }, () => {
  const env = piUpdateFixture('npm:chisle@3.0.0');

  const r = runCLI(['--only', 'pi', '--update', '--dry-run'], { env });
  assert.match(r.out, /would run: pi remove npm:chisle/);
  assert.match(r.out, /would run: pi install git:github\.com\/yungSNAT\/Puritanical/);
  assert.doesNotMatch(readText('home/pi.log'), /install|remove/, 'dry-run mutated');
});

test('--help advertises --update', () => {
  assert.match(runCLI(['--help']).out, /--update/);
});
// ── OpenCode + Hermes ────────────────────────────────────────────────────────
// Global agents. CHISLE_HOME redirects the home dir so tests never touch
// the real ~/.config/opencode or ~/.hermes. Runs on win32 too (no shell).

function isolatedOpencodeEnv(home, extra) {
  return { CHISLE_HOME: home, XDG_CONFIG_HOME: '', OPENCODE_CONFIG_DIR: '', ...extra };
}

function readShippedSkills(directory) {
  const previous = process.cwd();
  process.chdir(directory);
  try {
    return {
      chisle: readText('chisle/SKILL.md'),
      audit: readText('chisle-audit/SKILL.md'),
      help: readText('chisle-help/SKILL.md'),
      review: readText('chisle-review/SKILL.md'),
    };
  } finally {
    process.chdir(previous);
  }
}

function fakeOpencode(version) {
  const bin = absolutePath('home', 'bin');
  makeDir(bin, { recursive: true });
  // Windows only finds a bare `opencode` through PATHEXT, so the stub is a .cmd there.
  if (process.platform === 'win32') {
    writeText(under(bin, 'opencode.cmd'), `@echo off\r\nif "%1"=="--version" echo ${version}\r\n`);
  } else {
    writeText(under(bin, 'opencode'), `#!/bin/sh\n[ "$1" = "--version" ] && printf '${version}\\n'\n`, { mode: 0o755 });
  }
  return bin + path.delimiter + process.env.PATH;
}

test('--help and --list advertise opencode + hermes', () => {
  assert.match(runCLI(['--help']).out, /opencode/);
  assert.match(runCLI(['--help']).out, /hermes/);
  assert.match(runCLI(['--list']).out, /OpenCode/);
  assert.match(runCLI(['--list']).out, /Hermes/);
});

test('opencode v1 installation layout remains unchanged', () => {
  const home = absolutePath('home');
  makeDir('home');
  const env = isolatedOpencodeEnv(home, { PATH: fakeOpencode('1.19.5') });
  makeDir('home/.config/opencode/plugins/chisle', { recursive: true });
  writeText('home/.config/opencode/plugins/chisle/index.js', 'stale v2');
  let result = runCLI(['--only', 'opencode'], { env });
  assert.equal(result.status, 0, result.out);
  assert.ok(exists('home/.config/opencode/plugins/chisle.js'));
  assert.equal(exists('home/.config/opencode/plugins/chisle'), false);
  assert.ok(exists('home/.config/opencode/plugins/chisle-hooks/chisle-compress-output.js'));
  assert.equal(exists('home/.config/opencode/plugins/chisle'), false);

  result = runCLI(['--only', 'opencode', '--update'], { env });
  assert.equal(result.status, 0, result.out);
  assert.ok(exists('home/.config/opencode/plugins/chisle.js'));

  result = runCLI(['--uninstall', '--only', 'opencode'], { env });
  assert.equal(result.status, 0, result.out);
  assert.equal(exists('home/.config/opencode/plugins/chisle.js'), false);
});

test('opencode v2 install, update, dry-run, and uninstall use the v2 plugin directory', () => {
  const home = absolutePath('home');
  makeDir('home');
  const env = isolatedOpencodeEnv(home, { PATH: fakeOpencode('2.2.2') });
  let result = runCLI(['--only', 'opencode', '--dry-run'], { env });
  assert.equal(result.status, 0, result.out);
  assert.match(result.out, /plugins.*chisle/);
  assert.deepEqual(listDir('home'), ['bin']);

  makeDir('home/.config/opencode/plugins/chisle-hooks', { recursive: true });
  writeText('home/.config/opencode/plugins/chisle.js', 'legacy');
  result = runCLI(['--only', 'opencode'], { env });
  assert.equal(result.status, 0, result.out);
  assert.ok(exists('home/.config/opencode/plugins/chisle/index.js'));
  assert.equal(exists('home/.config/opencode/plugins/chisle.js'), false);
  assert.equal(exists('home/.config/opencode/plugins/chisle-hooks'), false);
  assert.ok(exists('home/.config/opencode/plugins/chisle/chisle-hooks/chisle-compress-output.js'));
  assert.equal(exists('home/.config/opencode/plugins/chisle.js'), false);

  writeText('home/.config/opencode/plugins/chisle/index.js', 'STALE');
  result = runCLI(['--only', 'opencode', '--update'], { env });
  assert.equal(result.status, 0, result.out);
  assert.notEqual(readText('home/.config/opencode/plugins/chisle/index.js'), 'STALE');

  result = runCLI(['--uninstall', '--only', 'opencode'], { env });
  assert.equal(result.status, 0, result.out);
  assert.equal(exists('home/.config/opencode/plugins/chisle'), false);
});

test('opencode install appends fenced ruleset, preserves user content, copies skills', () => {
  const home = absolutePath('home');
  makeDir('home');
  const env = isolatedOpencodeEnv(home);
  const agents = under(home, '.config', 'opencode', 'AGENTS.md');
  const skills = under(home, '.config', 'opencode', 'skills');
  makeDir(path.dirname(agents), { recursive: true });
  writeText(agents, '# my setup\n\nmy instructions.\n');

  const r = runCLI(['--only', 'opencode'], { env });
  assert.match(r.out, /installed:/);
  const txt = readText(agents);
  assert.ok(txt.startsWith('# my setup\n\nmy instructions.\n'), 'user instructions moved or clobbered');
  assert.ok(txt.includes('<!-- chisle-begin -->') && txt.includes('<!-- chisle-end -->'));
  assert.equal(txt.indexOf('<!-- chisle-begin -->'), txt.lastIndexOf('<!-- chisle-begin -->'), 'fence duplicated');
  assert.deepEqual(readShippedSkills(skills), readShippedSkills(under(__dirname, '..', 'skills')));

  // Compression plugin + its zero-dep core land in plugins/.
  const plugins = under(home, '.config', 'opencode', 'plugins');
  assert.ok(exists(under(plugins, 'chisle.js')), 'plugin not installed');
  assert.ok(exists(under(plugins, 'chisle-hooks', 'chisle-compress-output.js')), 'compressor core not installed');
  assert.ok(exists(under(plugins, 'chisle-hooks', 'chisle-config.js')), 'config core not installed');
  // The CJS core must stay CJS even though OpenCode's config dir is type:module,
  // which would otherwise propagate down and kill it on its first `require`.
  assert.equal(
    readJson(under(plugins, 'chisle-hooks', 'package.json')).type,
    'commonjs', 'compressor core not pinned to commonjs');

  // Second run: ruleset skipped, never duplicated; skills refreshed.
  const r2 = runCLI(['--only', 'opencode'], { env });
  assert.match(r2.out, /already contains chisle ruleset/);
  const txt2 = readText(agents);
  assert.equal(txt2.indexOf('<!-- chisle-begin -->'), txt2.lastIndexOf('<!-- chisle-begin -->'));
  assert.ok(txt2.startsWith('# my setup'), 'reinstall moved user content');

  // Stale fence refreshes under --update, user content survives.
  writeText(agents, txt2.replace(/# Chisle[\s\S]*<!-- chisle-end -->/, 'STALE\n<!-- chisle-end -->'));
  runCLI(['--only', 'opencode', '--update'], { env });
  const txt3 = readText(agents);
  assert.doesNotMatch(txt3, /STALE/);
  assert.ok(txt3.startsWith('# my setup'), 'update moved user content');

  // Uninstall removes the block + owned skills, restores user content.
  makeDir(under(skills, 'foreign'), { recursive: true });
  writeText(under(skills, 'foreign', 'SKILL.md'), 'not ours');
  const u = runCLI(['--uninstall', '--only', 'opencode'], { env });
  assert.match(u.out, /removed chisle block/);
  const after = readText(agents);
  assert.ok(after.startsWith('# my setup'), 'uninstall ate user content');
  assert.doesNotMatch(after, /chisle-begin/);
  assert.ok(!exists(under(skills, 'chisle')), 'owned skill left behind');
  assert.equal(readText(under(skills, 'foreign', 'SKILL.md')), 'not ours', 'foreign skill touched');
  assert.ok(!exists(under(plugins, 'chisle.js')), 'plugin left behind');
  assert.ok(!exists(under(plugins, 'chisle-hooks')), 'plugin core left behind');
});

for (const [name, extra] of [
  ['XDG_CONFIG_HOME', home => ({ XDG_CONFIG_HOME: under(home, 'xdg') })],
  ['OPENCODE_CONFIG_DIR', home => ({ XDG_CONFIG_HOME: under(home, 'ignored'), OPENCODE_CONFIG_DIR: under(home, 'xdg', 'opencode') })],
]) {
  test(`OpenCode install, update, stats, and uninstall share ${name}`, () => {
    const home = absolutePath('home');
    makeDir('home');
    const root = under(home, 'xdg', 'opencode');
    const env = isolatedOpencodeEnv(home, extra(home));
    let r = runCLI(['--only', 'opencode'], { env });
    assert.equal(r.status, 0, r.out);
    const agents = under(root, 'AGENTS.md');
    assert.match(readText(agents), /chisle-begin/);
    assert.ok(exists('home/xdg/opencode/plugins/chisle.js'));

    writeText(agents, readText(agents).replace(/# Chisle[\s\S]*<!-- chisle-end -->/, 'STALE\n<!-- chisle-end -->'));
    r = runCLI(['--only', 'opencode', '--update'], { env });
    assert.equal(r.status, 0, r.out);
    assert.doesNotMatch(readText(agents), /STALE/);

    writeText(under(root, '.chisle-compress-stats.json'), JSON.stringify({ savedChars: 1234, events: 2 }));
    r = runCLI(['--stats', '--only', 'opencode'], { env });
    assert.match(r.out, /1,234 chars/);
    makeDir(under(root, 'chisle-spill'));
    makeDir(under(root, 'chisle-dedup'));

    r = runCLI(['--uninstall', '--only', 'opencode'], { env });
    assert.equal(r.status, 0, r.out);
    assert.doesNotMatch(readText(agents), /chisle-begin/);
    assert.equal(exists('home/xdg/opencode/plugins/chisle.js'), false);
    assert.equal(exists(under(root, 'chisle-spill')), false);
    assert.equal(exists(under(root, 'chisle-dedup')), false);
    assert.equal(exists(under(root, '.chisle-compress-stats.json')), false);
  });
}

test('OpenCode directory detection uses the configured root', { skip: process.platform === 'win32' }, () => {
  const home = absolutePath('home');
  makeDir('home');
  const emptyPath = absolutePath('empty-path');
  makeDir('empty-path');
  const root = under(home, 'custom');
  makeDir(root);
  const r = runCLI(['--list'], {
    env: isolatedOpencodeEnv(home, { OPENCODE_CONFIG_DIR: root, PATH: emptyPath }),
  });
  assert.match(r.out, /✓ OpenCode/);
});

test('hermes install copies skills verbatim, uninstall prunes only owned', () => {
  const home = absolutePath('home');
  makeDir('home');
  const env = { CHISLE_HOME: home };
  const skills = under(home, '.hermes', 'skills');

  const r = runCLI(['--only', 'hermes'], { env });
  assert.match(r.out, /Hermes Agent detected/);
  assert.deepEqual(readShippedSkills(skills), readShippedSkills(under(__dirname, '..', 'skills')));

  makeDir(under(skills, 'mine'), { recursive: true });
  writeText(under(skills, 'mine', 'SKILL.md'), 'mine');
  const u = runCLI(['--uninstall', '--only', 'hermes'], { env });
  assert.match(u.out, /Uninstalled/);
  assert.ok(!exists(under(skills, 'chisle')));
  assert.equal(readText(under(skills, 'mine', 'SKILL.md')), 'mine');
});

test('opencode+hermes dry-run changes nothing on disk', () => {
  const home = absolutePath('home');
  makeDir('home');
  const env = isolatedOpencodeEnv(home);
  runCLI(['--only', 'opencode', '--only', 'hermes', '--dry-run'], { env });
  assert.deepEqual(listDir(home), [], 'dry-run wrote into CHISLE_HOME');
});

test('a fence missing its end marker is left alone, not truncated', () => {
  // A begin marker with no end marker means a hand edit or a cut-short write.
  // Everything after it is user content of unknown extent, so both refresh and
  // uninstall must refuse. Slicing to the end marker unconditionally would eat
  // the rest of the user's global instructions.
  const home = absolutePath('home');
  makeDir('home');
  const env = isolatedOpencodeEnv(home);
  const body = 'my rules\n\n<!-- chisle-begin -->\nstale\n\nKEEP THIS\n';

  const codex = under(home, '.codex');
  const opencode = under(home, '.config', 'opencode');
  makeDir(codex);
  makeDir(opencode, { recursive: true });
  for (const [id, directory] of [['codex', codex], ['opencode', opencode]]) {
    process.chdir(directory);
    writeText('AGENTS.md', body);
    const install = runCLI(['--only', id, '--force'], { env, cwd: home });
    assert.match(install.out, /no chisle-end/, id + ': no warning');
    assert.equal(readText('AGENTS.md'), body, id + ': --force rewrote a malformed fence');

    runCLI(['--uninstall', '--only', id], { env, cwd: home });
    assert.equal(readText('AGENTS.md'), body, id + ': uninstall ate user content');
  }
});
test('codex fenced install still works via shared helper (refactor guard)', () => {
  const home = absolutePath('home');
  makeDir('home');
  const env = { CHISLE_HOME: home };
  const agents = under(home, '.codex', 'AGENTS.md');

  runCLI(['--only', 'codex'], { env });
  assert.ok(readText(agents).includes('<!-- chisle-begin -->'));
  const r = runCLI(['--only', 'codex'], { env });
  assert.match(r.out, /already contains chisle ruleset/);
  runCLI(['--uninstall', '--only', 'codex'], { env });
  assert.doesNotMatch(readText(agents), /chisle-begin/);
});

// A shell-out install whose CLI is missing must be reported as a failure, not a
// success. spawnSync returns { status: null, error: ENOENT } for a missing
// binary, and the old `(r.status || 0) === 0` check read that null as 0 — so a
// machine without `gemini`/`pi` was told the install had succeeded, and the
// process still exited 0. Guard both the message and the exit code.
test('shell-out install with the CLI absent reports failure, not success', () => {
  const home = absolutePath('home');
  makeDir('home');
  // A PATH with nothing on it: every `gemini`/`pi` lookup resolves to ENOENT.
  const emptyBin = absolutePath('empty-bin');
  makeDir('empty-bin');
  for (const agent of ['gemini', 'pi']) {
    const r = runCLI(['--only', agent, '--force'], {
      env: { HOME: home, USERPROFILE: home, PATH: emptyBin },
    });
    const lines = r.out.split(/\r?\n/).map(line => line.trim().replace(/\s+/g, ' '));
    assert.ok(lines.some(line => line.includes('failed: ' + agent)),
      `${agent}: expected a failure line, got:\n${r.out}`);
    assert.ok(!lines.some(line => line.includes('installed: ' + agent)),
      `${agent}: reported success despite a missing CLI`);
    assert.equal(r.status, 1, `${agent}: expected exit 1 on failure`);
  }
});

// --dry-run must keep reporting success: nothing is executed, so there is no
// exit status to judge, and run() short-circuits to { status: 0 }.
test('--dry-run still reports success for shell-out agents', () => {
  const home = absolutePath('home');
  makeDir('home');
  const r = runCLI(['--only', 'gemini', '--dry-run'], { env: { HOME: home, USERPROFILE: home } });
  assert.match(r.out, /installed:\s+gemini/);
  assert.equal(r.status, 0);
});

// `claude plugin install` skips an installed plugin, so --update must use
// `claude plugin update`, or users stay on the old version.
test('REGRESSION: --update upgrades an installed Claude plugin via plugin update', { skip: process.platform === 'win32' }, () => {
  const cfg = absolutePath('config');
  makeDir('config');
  const binDir = absolutePath('bin');
  makeDir('bin');
  const log = under(binDir, 'calls.log');
  writeText(under(binDir, 'claude'), `#!/bin/sh\necho "$@" >> ${JSON.stringify(log)}\necho chisle\nexit 0\n`, { mode: 0o755 });
  runCLI(['--update', '--only', 'claude', '--config-dir', cfg], { env: { PATH: binDir + path.delimiter + process.env.PATH } });
  const calls = readText(log);
  assert.match(calls, /^plugin update chisle@chisle$/m);
  assert.doesNotMatch(calls, /^plugin install /m);
});
