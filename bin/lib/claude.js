/* global module, process, require -- Node.js runtime */
// puritanical installer: Claude Code. Plugin (marketplace add + install) first,
// standalone hooks + settings.json merge + statusline badge as the fallback.
// Every path is under the Claude config dir (--config-dir, CLAUDE_CONFIG_DIR
// or ~/.claude) or this package's hooks/.

'use strict';

const fs = require('fs');
const path = require('path');
const SETTINGS = require('./settings');
const ENV = require('./env');
const FENCE = require('./fence');
const { OLD_NAME, isOldStatusLine } = require('../../hooks/puritanical-config');

const HOOK_FILES = ['package.json', 'puritanical-config.js', 'puritanical-activate.js',
  'puritanical-mode.js', 'puritanical-mode-tracker.js', 'puritanical-compress-output.js', 'puritanical-command-filters.js',
  'puritanical-run.js', 'puritanical-statusline.sh', 'puritanical-statusline.ps1'];

function settingsPathFor(opts) { return path.join(ENV.claudeDir(opts), 'settings.json'); } // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal

// Hook commands from this package, under the current or the pre-4.0.0 name.
function hasPuritanicalHookCommand(settings) {
  const hooks = (settings && settings.hooks) || {};
  const list = v => (Array.isArray(v) ? v : []);
  return Object.values(hooks).some(entries => list(entries).some(entry =>
    list(entry && entry.hooks).some(h => h && typeof h.command === 'string' &&
      (h.command.includes('puritanical-') || h.command.includes(OLD_NAME + '-')))));
}

function claudeInstalled(opts) {
  if (ENV.hasCmd('claude') && (ENV.listsPuritanical('claude', ['plugin', 'list']) ||
      ENV.listsPuritanical('claude', ['plugin', 'list'], ENV.OLD_LISTED))) return true;
  return hasPuritanicalHookCommand(SETTINGS.readSettings(settingsPathFor(opts)));
}

// Installed copies of a plugin as { scope, project }, from `claude plugin list
// --json`. A CLI without --json falls back to the text list: one copy, at the
// default scope.
function pluginCopies(id, listed) {
  const r = ENV.capture('claude', ['plugin', 'list', '--json']);
  try {
    const list = JSON.parse(r.status === 0 ? r.stdout : '');
    if (Array.isArray(list)) return list.filter(p => p && p.id === id).map(p => ({ scope: p.scope, project: p.projectPath }));
  } catch {}
  return ENV.listsPuritanical('claude', ['plugin', 'list'], listed) ? [{}] : [];
}

// Uninstalls each copy of a plugin this run may change: user scope, and local
// scope in the current project. Project scope is recorded in a settings file
// shared with a team, so that copy, and any that fails, is left with the
// command to run. Returns the counts removed and left.
function uninstallPlugin(ctx, id, listed) {
  let removed = 0;
  let left = 0;
  for (const { scope, project } of pluginCopies(id, listed)) {
    const args = ['plugin', 'uninstall', id].concat(scope ? ['--scope', scope] : []);
    const here = !project || path.resolve(project) === process.cwd();
    if (scope !== 'project' && here && ENV.ranOk(ENV.run('claude', args, ctx.opts.dryRun))) { removed++; continue; }
    left++;
    ctx.warn(`  ${id} is still installed at ${scope || 'user'} scope; remove it with: ` +
      (here ? '' : `cd ${JSON.stringify(project)} && `) + 'claude ' + args.join(' '));
  }
  return { removed, left };
}

// Removes a configured marketplace; returns 1 when it went.
function removeMarketplace(ctx, name) {
  const r = ENV.capture('claude', ['plugin', 'marketplace', 'list', '--json']);
  let names = [];
  try { names = JSON.parse(r.stdout).map(m => m && m.name); } catch {}
  if (!names.includes(name)) return 0;
  return ENV.ranOk(ENV.run('claude', ['plugin', 'marketplace', 'remove', name], ctx.opts.dryRun)) ? 1 : 0;
}

// An install before 4.0.0 added the plugin under its old name, from a marketplace
// of that name. Both go, so the two plugins never run side by side; the
// marketplace stays while any copy of the old plugin does.
function removeOldPlugin(ctx) {
  if (uninstallPlugin(ctx, ENV.OLD_PLUGIN, ENV.OLD_LISTED).left) ctx.warn(`  kept the ${OLD_NAME} marketplace while the old plugin is installed`);
  else removeMarketplace(ctx, OLD_NAME);
}

// A status line that runs the old badge script among other commands stays
// as the user wrote it; this says what to change.
function noteOldStatusLine(note, sl, settingsPath) {
  if (sl && !isOldStatusLine(sl) && JSON.stringify(sl).includes(OLD_NAME + '-statusline')) {
    note(`  left your status line in ${settingsPath}: it runs the pre-4.0.0 ${OLD_NAME}-statusline script; point it at puritanical-statusline`);
  }
}

// Hook entries and the hooks folder a standalone install before 4.0.0 wired.
// Returns the number of settings entries removed.
function dropOldStandalone(ctx, settings) {
  const removed = SETTINGS.removeHooks(settings, OLD_NAME + '-');
  const dir = path.join(ENV.claudeDir(ctx.opts), OLD_NAME + '-hooks'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal, rules_lgpl_javascript_traversal_rule-join-resolve-path-traversal
  if (FENCE.exists(dir) && !ctx.opts.dryRun) FENCE.removeTree(dir);
  return removed;
}

// Returns true when the plugin is installed (already, or by this run).
function installPlugin(ctx) {
  const { note, warn, opts, results } = ctx;
  if (!opts.force && ENV.listsPuritanical('claude', ['plugin', 'list'])) {
    note('  puritanical plugin already installed (use --force to reinstall)');
    results.skipped.push(['claude', 'plugin already installed']);
    return true;
  }
  // `plugin install` is a no-op for an already-installed plugin, so an
  // upgrade (--update / --force) has to go through `plugin update`.
  const present = opts.force && /puritanical/i.test(ENV.capture('claude', ['plugin', 'list']).stdout || '');
  const r1 = ENV.run('claude', ['plugin', 'marketplace', 'add', ENV.REPO], opts.dryRun);
  const r2 = ENV.run('claude', ['plugin', present ? 'update' : 'install', 'puritanical@puritanical'], opts.dryRun);
  if (ENV.ranOk(r1) && ENV.ranOk(r2)) { results.installed.push('claude'); return true; }
  warn('  claude plugin install failed, falling back to standalone hooks');
  return false;
}

// Plugin path won, so drop any standalone wiring left by an earlier run, or
// both copies of every hook run on each event and the PostToolUse pair
// dedups first-seen output against itself.
function dropStandaloneHooks(ctx) {
  const { note, opts } = ctx;
  note('  hooks: plugin manifest handles SessionStart + UserPromptSubmit + PreToolUse + PostToolUse');
  const settingsPath = settingsPathFor(opts);
  const settings = SETTINGS.readSettings(settingsPath);
  if (!settings) return;
  noteOldStatusLine(note, settings.statusLine, settingsPath);
  const oldLine = isOldStatusLine(settings.statusLine);
  if (oldLine) delete settings.statusLine;
  if (SETTINGS.removeHooks(settings, 'puritanical-') + dropOldStandalone(ctx, settings) === 0 && !oldLine) return;
  if (!opts.dryRun) {
    backupSettings(settingsPath);
    SETTINGS.validateHookFields(settings);
    SETTINGS.writeSettings(settingsPath, settings);
  }
  note('  removed superseded standalone hook entries from settings.json');
}

function installClaude(ctx) {
  const { say, note, results } = ctx;
  results.detected++;
  say('→ Claude Code detected');
  let pluginOK = false;
  if (ENV.hasCmd('claude')) {
    removeOldPlugin(ctx);
    pluginOK = installPlugin(ctx);
  }
  else note('  claude CLI not on PATH, wiring standalone hooks in settings.json');

  if (pluginOK) dropStandaloneHooks(ctx);
  else {
    const r = installClaudeHooks(ctx);
    if (r === 'ok') results.installed.push('claude-hooks');
    else results.failed.push(['claude-hooks', r]);
  }
  process.stdout.write('\n');
}

function copyHookFiles(hooksDst) {
  const hooksSrc = path.join(ENV.REPO_ROOT, 'hooks');
  FENCE.mkdirp(hooksDst);
  for (const f of HOOK_FILES) {
    const s = path.join(hooksSrc, f);
    if (FENCE.exists(s)) FENCE.copyFile(s, path.join(hooksDst, f)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  }
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- The statusline script just copied into the Claude config directory.
  try { fs.chmodSync(path.join(hooksDst, 'puritanical-statusline.sh'), 0o700); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

function backupSettings(settingsPath) {
  const bak = settingsPath + '.bak';
  if (FENCE.exists(settingsPath) && !FENCE.exists(bak)) { try { FENCE.copyFile(settingsPath, bak); } catch {} }
}

function wireHooks(settings, hooksDst) {
  const node = process.execPath;
  const hook = name => `"${node}" "${path.join(hooksDst, name)}"`; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  SETTINGS.addCommandHook(settings, 'SessionStart',
    { command: hook('puritanical-activate.js'), marker: 'puritanical-activate', timeout: 5, statusMessage: 'Loading puritanical mode...' });
  SETTINGS.addCommandHook(settings, 'UserPromptSubmit',
    { command: hook('puritanical-mode-tracker.js'), marker: 'puritanical-mode-tracker', timeout: 5, statusMessage: 'Tracking puritanical mode...' });
  if (!ENV.IS_WIN) {
    SETTINGS.addCommandHook(settings, 'PreToolUse',
      { command: hook('puritanical-run.js'), marker: 'puritanical-run', timeout: 5, matcher: 'Bash' });
  }
  SETTINGS.addCommandHook(settings, 'PostToolUse',
    { command: hook('puritanical-compress-output.js'), marker: 'puritanical-compress-output', timeout: 10,
      matcher: 'Bash|Agent|WebFetch|WebSearch|Grep|Glob|mcp__.*', statusMessage: 'Compressing tool output...' });
}

function statusLineCommand(hooksDst) {
  if (!ENV.IS_WIN) return `bash "${path.join(hooksDst, 'puritanical-statusline.sh')}"`; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const psHost = ENV.hasCmd('pwsh') ? 'pwsh' : 'powershell';
  return `${psHost} -NoProfile -ExecutionPolicy Bypass -File "${path.join(hooksDst, 'puritanical-statusline.ps1')}"`; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

function configureStatusLine(settings, hooksDst) {
  if (!settings.statusLine || isOldStatusLine(settings.statusLine)) {
    settings.statusLine = { type: 'command', command: statusLineCommand(hooksDst) };
    process.stdout.write('  statusline badge configured.\n');
    return;
  }
  const existing = typeof settings.statusLine === 'string' ? settings.statusLine : (settings.statusLine.command || '');
  if (existing.includes('puritanical-statusline')) process.stdout.write('  statusline badge already configured.\n');
  else process.stdout.write('  NOTE: existing statusline detected, PURITANICAL badge NOT added (see docs/install-windows.md).\n');
}

// Standalone wiring: copy hooks into <configDir>/puritanical-hooks/ and merge
// settings.json. Used when the plugin path is unavailable.
function installClaudeHooks(ctx) {
  const { opts, warn, note } = ctx;
  const cfg = ENV.claudeDir(opts);
  const hooksDst = path.join(cfg, 'puritanical-hooks'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const settingsPath = path.join(cfg, 'settings.json'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal

  if (opts.dryRun) {
    note(`  would copy ${HOOK_FILES.length} hook files → ${hooksDst}`);
    note(`  would merge SessionStart + UserPromptSubmit + PreToolUse + PostToolUse + statusline → ${settingsPath}`);
    return 'ok';
  }

  copyHookFiles(hooksDst);
  const settings = SETTINGS.readSettings(settingsPath);
  if (settings === null) { warn('  settings.json unparseable; not touching it.'); return 'settings.json unparseable'; }
  backupSettings(settingsPath);
  noteOldStatusLine(note, settings.statusLine, settingsPath);
  dropOldStandalone(ctx, settings);
  wireHooks(settings, hooksDst);
  configureStatusLine(settings, hooksDst);
  SETTINGS.validateHookFields(settings);
  SETTINGS.writeSettings(settingsPath, settings);
  process.stdout.write(`  hooks wired in ${settingsPath}\n`);
  return 'ok';
}

module.exports = { installClaude, claudeInstalled, uninstallPlugin, removeMarketplace };
