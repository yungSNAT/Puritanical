/* global module, process, require -- Node.js runtime */
// chisle installer: every agent except Claude Code. Global targets live under
// the user's home or the agent's configured root (CHISLE_HOME, OpenCode config
// dir, COPILOT_HOME); project rules go into the current directory. Sources
// are files in this package.

'use strict';

const path = require('path');
const ENV = require('./env');
const FENCE = require('./fence');
const { piSources } = require('./uninstall');

const COMPRESSOR = ['chisle-compress-output.js', 'chisle-command-filters.js', 'chisle-config.js'];
const COMMONJS_PACKAGE = '{ "type": "commonjs" }\n';

function blankLine() { process.stdout.write('\n'); }
function failure(e, fallback) { return (e && e.message) || fallback; }

// ── Pi ──────────────────────────────────────────────────────────────────────
function installPi(ctx) {
  const { say, note, opts, results } = ctx;
  results.detected++;
  say('→ Pi detected');
  if (!opts.force && piSources(ENV.capture('pi', ['list']).stdout).length) {
    note('  chisle package already installed (use --force)');
    results.skipped.push(['pi', 'already installed']); blankLine(); return;
  }
  const r = ENV.run('pi', ['install', 'npm:chisle'], opts.dryRun);
  if (ENV.ranOk(r)) results.installed.push('pi');
  else results.failed.push(['pi', 'pi package install failed']);
  blankLine();
}

// ── Gemini ──────────────────────────────────────────────────────────────────
function installGemini(ctx) {
  const { say, note, opts, results } = ctx;
  results.detected++;
  say('→ Gemini CLI detected');
  if (!opts.force && ENV.listsChisle('gemini', ['extensions', 'list'])) {
    note('  chisle extension already installed (use --force)');
    results.skipped.push(['gemini', 'already installed']); blankLine(); return;
  }
  const r = ENV.run('gemini', ['extensions', 'install', `https://github.com/${ENV.REPO}`], opts.dryRun);
  if (ENV.ranOk(r)) results.installed.push('gemini');
  else results.failed.push(['gemini', 'gemini extensions install failed']);
  blankLine();
}

// ── Codex (fenced ruleset in ~/.codex/AGENTS.md) ────────────────────────────
function installCodex(ctx) {
  const { say, note, opts, results } = ctx;
  results.detected++;
  say('→ Codex detected');
  const target = path.join(ENV.homeDir(), '.codex', 'AGENTS.md');

  if (opts.dryRun) { note('  would write chisle ruleset to ' + target); results.installed.push('codex'); blankLine(); return; }

  try {
    const st = FENCE.writeFencedRuleset(target, opts, note);
    if (st === 'installed') results.installed.push('codex');
    else results.skipped.push(['codex', 'already present']);
  } catch (e) { results.failed.push(['codex', failure(e, 'write failed')]); }
  blankLine();
}

// ── OpenCode ─────────────────────────────────────────────────────────────
// Global AGENTS.md carries the always-on output axis next to existing user
// instructions; the bundled skills copy under the same OpenCode config root
// for on-demand loading via the native skill tool
// (the global skills dir is scanned by default, so skills.paths needs no
// edit and no second system message is added).
// Copies the version-matched OpenCode plugin and shared compressor core.
// V1 gets plugins/chisle.js; V2 gets the directory plugin plugins/chisle/.
// The source entrypoints stay separate and both load the same CommonJS core.
// chisle-hooks/package.json pins that core as CommonJS under type:module roots.
function opencodePluginFiles(entryDir, hooksDir, v2) {
  const entry = v2
    ? [['.opencode/plugins-v2/chisle/index.mjs', path.join(entryDir, 'index.js')], // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      ['.opencode/plugins-v2/chisle/setup.cjs', path.join(entryDir, 'setup.cjs')]] // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    : [['.opencode/plugins/chisle.mjs', path.join(entryDir, 'chisle.js')]]; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return entry.concat(COMPRESSOR.map(f => ['hooks/' + f, path.join(hooksDir, f)])); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

function copyOpencodePlugin(root, major) {
  const pluginDir = path.join(root, 'plugins'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const v2 = major >= 2;
  const entryDir = v2 ? path.join(pluginDir, 'chisle') : pluginDir; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const hooksDir = path.join(entryDir, 'chisle-hooks'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const files = opencodePluginFiles(entryDir, hooksDir, v2);
  FENCE.mkdirp(hooksDir);
  for (const [src, dst] of files) FENCE.copyFile(path.join(ENV.REPO_ROOT, src), dst);
  FENCE.writeText(path.join(hooksDir, 'package.json'), COMMONJS_PACKAGE); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return files.length + 1;
}

function installOpencode(ctx) {
  const { say, note, opts, results } = ctx;
  results.detected++;
  say('→ OpenCode detected');
  const root = ENV.opencodeDir();
  const target = path.join(root, 'AGENTS.md');
  const skillsDst = path.join(root, 'skills');
  const major = ENV.opencodeMajor();
  const pluginDst = path.join(root, 'plugins', major >= 2 ? 'chisle' : '');

  if (opts.dryRun) {
    note('  would write chisle ruleset to ' + target);
    note('  would copy skills to ' + skillsDst);
    note('  would copy compression plugin to ' + pluginDst);
    results.installed.push('opencode');
    blankLine();
    return;
  }

  try {
    const st = FENCE.writeFencedRuleset(target, opts, note);
    const stale = major >= 2 ? ['chisle.js', 'chisle-hooks'] : ['chisle'];
    for (const rel of stale) FENCE.removeTree(path.join(root, 'plugins', rel));
    const n = FENCE.copySkills(skillsDst, opts);
    process.stdout.write('  installed: ' + n + ' skill file(s) to ' + skillsDst + '\n');
    const p = copyOpencodePlugin(root, major);
    process.stdout.write('  installed: ' + p + ' plugin file(s) to ' + pluginDst + '\n');
    if (st === 'installed') results.installed.push('opencode');
    else results.skipped.push(['opencode', 'ruleset already present; skills + plugin refreshed']);
  } catch (e) { results.failed.push(['opencode', failure(e, 'write failed')]); }
  blankLine();
}

// ── Copilot CLI ─────────────────────────────────────────────────────────────
// Rules: fenced block in ~/.copilot/copilot-instructions.md, Copilot CLI's
// user-level instructions file. Input axis: a user-level postToolUse hook
// (~/.copilot/hooks/chisle.json, no workspace admission needed) running the
// shared compressor, which already speaks Copilot's camelCase payload and
// answers with modifiedResult. Verified live on Copilot CLI 1.0.95: a
// 13.9k-char shell result reached the model at ~100 lines with the mid-log
// error line and the `<shellId: 0 completed with exit code 0>` tail intact.
function copilotHookConfig(hookPath) {
  return JSON.stringify({
    version: 1,
    hooks: {
      postToolUse: [{
        type: 'command',
        bash: 'node "' + hookPath + '"',
        powershell: 'node "' + hookPath + '"',
        timeoutSec: 10,
      }],
    },
  }, null, 2) + '\n';
}

function copyCompressor(hooksDir) {
  FENCE.mkdirp(hooksDir);
  for (const f of COMPRESSOR) FENCE.copyFile(path.join(ENV.REPO_ROOT, 'hooks', f), path.join(hooksDir, f)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  FENCE.writeText(path.join(hooksDir, 'package.json'), COMMONJS_PACKAGE); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

function installCopilotCli(ctx) {
  const { say, note, opts, results } = ctx;
  results.detected++;
  say('→ Copilot CLI detected');
  const root = ENV.copilotDir();
  const rules = path.join(root, 'copilot-instructions.md');
  const hooksDir = path.join(root, 'chisle-hooks');
  const hookFile = path.join(root, 'hooks', 'chisle.json');

  if (opts.dryRun) {
    note('  would write chisle ruleset to ' + rules);
    note('  would copy compressor to ' + hooksDir);
    note('  would write postToolUse hook ' + hookFile);
    results.installed.push('copilot-cli');
    blankLine();
    return;
  }

  try {
    const st = FENCE.writeFencedRuleset(rules, opts, note);
    copyCompressor(hooksDir);
    const hookExisted = FENCE.exists(hookFile);
    if (!hookExisted || opts.force) {
      FENCE.mkdirp(path.dirname(hookFile));
      FENCE.writeText(hookFile, copilotHookConfig(path.join(hooksDir, 'chisle-compress-output.js')));
      process.stdout.write('  installed: ' + hookFile + '\n');
    }
    if (st === 'installed' || !hookExisted) results.installed.push('copilot-cli');
    else results.skipped.push(['copilot-cli', 'already present; compressor refreshed']);
  } catch (e) { results.failed.push(['copilot-cli', failure(e, 'write failed')]); }
  blankLine();
}

// ── Hermes ──────────────────────────────────────────────────────────
// Agent-Skills host: bundled skills land verbatim in ~/.hermes/skills, where
// Hermes discovers them as /chisle slash commands. No ruleset is injected:
// Hermes already reads project AGENTS.md, which carries the always-on axis
// when the repo ships it. Portable: honors CHISLE_HOME, no hardcoded paths.
function installHermes(ctx) {
  const { say, opts, results } = ctx;
  results.detected++;
  say('→ Hermes Agent detected');
  const skillsDst = path.join(ENV.homeDir(), '.hermes', 'skills');

  if (opts.dryRun) {
    process.stdout.write('  would copy skills to ' + skillsDst + '\n');
    results.installed.push('hermes');
    blankLine();
    return;
  }

  try {
    const n = FENCE.copySkills(skillsDst, opts);
    process.stdout.write('  installed: ' + n + ' skill file(s) to ' + skillsDst + '\n');
    results.installed.push('hermes');
  } catch (e) { results.failed.push(['hermes', failure(e, 'copy failed')]); }
  blankLine();
}

// ── Project-scoped rule agents (Cursor/Windsurf/Cline/Kiro/Copilot/Antigravity)
// prov.rule is a fixed relative path from the installer's provider table.
function installProjectRule(ctx, prov) {
  const { say, note, warn, opts, results } = ctx;
  results.detected++;
  say(`→ ${prov.label} detected`);
  const src = path.join(ENV.REPO_ROOT, prov.rule); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const dst = path.join(process.cwd(), prov.rule); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal

  if (!FENCE.exists(src)) { warn(`  missing source rule ${prov.rule} (run npm run build:rules)`); results.failed.push([prov.id, 'source rule missing']); blankLine(); return; }

  if (opts.dryRun) { note(`  would write ${dst} (project-scoped)`); results.installed.push(prov.id); blankLine(); return; }

  if (FENCE.exists(dst) && !opts.force) {
    note(`  ${prov.rule} already in this project (--force to overwrite)`);
    results.skipped.push([prov.id, 'already in project']); blankLine(); return;
  }
  try {
    FENCE.mkdirp(path.dirname(dst));
    FENCE.copyFile(src, dst);
    process.stdout.write(`  installed: ${dst}  (project-scoped)\n`);
    results.installed.push(prov.id);
  } catch (e) { results.failed.push([prov.id, failure(e, 'copy failed')]); }
  blankLine();
}

const INSTALLERS = new Map([
  ['pi', installPi],
  ['gemini', installGemini],
  ['codex', installCodex],
  ['opencode', installOpencode],
  ['hermes', installHermes],
  ['copilot-cli', installCopilotCli],
]);

module.exports = { INSTALLERS, installProjectRule };
