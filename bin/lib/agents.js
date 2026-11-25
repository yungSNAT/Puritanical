/* global module, process, require -- Node.js runtime */
// puritanical installer: every agent except Claude Code. Global targets live under
// the user's home or the agent's configured root (PURITANICAL_HOME, OpenCode config
// dir, COPILOT_HOME); project rules go into the current directory. Sources
// are files in this package.

'use strict';

const path = require('path');
const ENV = require('./env');
const FENCE = require('./fence');
const { piSources } = require('./uninstall');
const crypto = require('crypto');
const { NAME, OLD_NAME, isLink, moveIfAbsent } = require('../../hooks/puritanical-config');
const OLD_RULES = require('./old-rules');

const COMPRESSOR = ['puritanical-compress-output.js', 'puritanical-command-filters.js', 'puritanical-config.js',
  'puritanical-core-wasm.cjs', 'puritanical-core.wasm'];
const COMMONJS_PACKAGE = '{ "type": "commonjs" }\n';

function blankLine() { process.stdout.write('\n'); }
function failure(e, fallback) { return (e && e.message) || fallback; }

// ── Pi ──────────────────────────────────────────────────────────────────────
function installPi(ctx) {
  const { say, note, warn, opts, results } = ctx;
  results.detected++;
  say('→ Pi detected');
  // Sources from before the fork took the name (npm:chisle, upstream git) go
  // first, so the old package never loads beside this one.
  const sources = piSources(ENV.capture('pi', ['list']).stdout);
  const ours = s => s.toLowerCase().includes(ENV.REPO.toLowerCase());
  for (const old of sources.filter(s => !ours(s))) {
    if (!ENV.ranOk(ENV.run('pi', ['remove', old], opts.dryRun))) {
      warn('  could not remove old pi package ' + old + '; leaving pi unchanged');
      results.failed.push(['pi', 'could not remove ' + old]); blankLine(); return;
    }
  }
  if (!opts.force && sources.some(ours)) {
    note('  puritanical package already installed (use --force)');
    results.skipped.push(['pi', 'already installed']); blankLine(); return;
  }
  const r = ENV.run('pi', ['install', `git:github.com/${ENV.REPO}`], opts.dryRun);
  if (ENV.ranOk(r)) results.installed.push('pi');
  else results.failed.push(['pi', 'pi package install failed']);
  blankLine();
}

// ── Gemini ──────────────────────────────────────────────────────────────────
function installGemini(ctx) {
  const { say, note, warn, opts, results } = ctx;
  results.detected++;
  say('→ Gemini CLI detected');
  if (ENV.listsPuritanical('gemini', ['extensions', 'list'], ENV.OLD_LISTED)) {
    if (!ENV.ranOk(ENV.run('gemini', ['extensions', 'uninstall', OLD_NAME], opts.dryRun))) {
      warn('  could not remove old Gemini CLI extension; leaving Gemini CLI unchanged');
      results.failed.push(['gemini', 'could not remove old extension']); blankLine(); return;
    }
  }
  if (!opts.force && ENV.listsPuritanical('gemini', ['extensions', 'list'])) {
    note('  puritanical extension already installed (use --force)');
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

  if (opts.dryRun) { note('  would write puritanical ruleset to ' + target); results.installed.push('codex'); blankLine(); return; }

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
// V1 gets plugins/puritanical.js; V2 gets the directory plugin plugins/puritanical/.
// The source entrypoints stay separate and both load the same CommonJS core.
// puritanical-hooks/package.json pins that core as CommonJS under type:module roots.
function opencodePluginFiles(entryDir, hooksDir, v2) {
  const entry = v2
    ? [['.opencode/plugins-v2/puritanical/index.mjs', path.join(entryDir, 'index.js')], // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      ['.opencode/plugins-v2/puritanical/setup.cjs', path.join(entryDir, 'setup.cjs')]] // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    : [['.opencode/plugins/puritanical.mjs', path.join(entryDir, 'puritanical.js')]]; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return entry.concat(COMPRESSOR.map(f => ['hooks/' + f, path.join(hooksDir, f)])); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

function copyOpencodePlugin(root, major) {
  const pluginDir = path.join(root, 'plugins'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const v2 = major >= 2;
  const entryDir = v2 ? path.join(pluginDir, 'puritanical') : pluginDir; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const hooksDir = path.join(entryDir, 'puritanical-hooks'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
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
  const pluginDst = path.join(root, 'plugins', major >= 2 ? 'puritanical' : '');

  if (opts.dryRun) {
    note('  would write puritanical ruleset to ' + target);
    note('  would copy skills to ' + skillsDst);
    note('  would copy compression plugin to ' + pluginDst);
    results.installed.push('opencode');
    blankLine();
    return;
  }

  try {
    const st = FENCE.writeFencedRuleset(target, opts, note);
    // The other OpenCode layout's plugin, and any pre-4.0.0 one, would run twice.
    const stale = (major >= 2 ? ['puritanical.js', 'puritanical-hooks'] : ['puritanical'])
      .concat([OLD_NAME + '.js', OLD_NAME + '-hooks', OLD_NAME]);
    for (const rel of stale) FENCE.removeTree(path.join(root, 'plugins', rel));
    FENCE.pruneSkills(skillsDst, false, FENCE.oldSkillNames());
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
// (~/.copilot/hooks/puritanical.json, no workspace admission needed) running the
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
  const hooksDir = path.join(root, 'puritanical-hooks');
  const hookFile = path.join(root, 'hooks', 'puritanical.json');

  if (opts.dryRun) {
    note('  would write puritanical ruleset to ' + rules);
    note('  would copy compressor to ' + hooksDir);
    note('  would write postToolUse hook ' + hookFile);
    results.installed.push('copilot-cli');
    blankLine();
    return;
  }

  try {
    const st = FENCE.writeFencedRuleset(rules, opts, note);
    // A pre-4.0.0 hook would compress every result a second time.
    for (const old of [path.join('hooks', OLD_NAME + '.json'), OLD_NAME + '-hooks']) FENCE.removeTree(path.join(root, old));
    copyCompressor(hooksDir);
    const hookExisted = FENCE.exists(hookFile);
    if (!hookExisted || opts.force) {
      FENCE.mkdirp(path.dirname(hookFile));
      FENCE.writeText(hookFile, copilotHookConfig(path.join(hooksDir, 'puritanical-compress-output.js')));
      process.stdout.write('  installed: ' + hookFile + '\n');
    }
    if (st === 'installed' || !hookExisted) results.installed.push('copilot-cli');
    else results.skipped.push(['copilot-cli', 'already present; compressor refreshed']);
  } catch (e) { results.failed.push(['copilot-cli', failure(e, 'write failed')]); }
  blankLine();
}

// ── Hermes ──────────────────────────────────────────────────────────
// Agent-Skills host: bundled skills land verbatim in ~/.hermes/skills, where
// Hermes discovers them as /puritanical slash commands. No ruleset is injected:
// Hermes already reads project AGENTS.md, which carries the always-on axis
// when the repo ships it. Portable: honors PURITANICAL_HOME, no hardcoded paths.
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
    FENCE.pruneSkills(skillsDst, false, FENCE.oldSkillNames());
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

  removeOldRule(note, prov.rule);
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

// The copy of this rule an install before 4.0.0 wrote under the old file name.
// Removed only while it is byte for byte (line endings aside) a copy some
// release shipped. Any other content may hold the user's additions, so it moves
// aside to <file>.chisle-bak (formerly Chisle), where the agent no longer loads it beside the new
// rule; an existing backup is never overwritten, so a later one takes -2, -3 and so on.
function removeOldRule(note, rule) {
  const old = path.join(process.cwd(), rule.replace(NAME, OLD_NAME)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  if (old === path.join(process.cwd(), rule) || !FENCE.exists(old)) return; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const digest = crypto.createHash('sha256').update(FENCE.readText(old).replace(/\r/g, '')).digest('hex');
  if (OLD_RULES.has(digest)) {
    FENCE.removeTree(old);
    note('  removed the pre-4.0.0 rule ' + old + ', an unedited copy');
    return;
  }
  const base = old + '.' + OLD_NAME + '-bak';
  let n = 1;
  while (FENCE.exists(n === 1 ? base : base + '-' + n) || isLink(n === 1 ? base : base + '-' + n)) n++;
  const candidate = n === 1 ? base : base + '-' + n;
  const bak = moveIfAbsent(old, candidate) ? candidate : null;
  note(bak ? '  moved the pre-4.0.0 rule ' + old + ' to ' + bak + ': it differs from every released copy, so your additions are there; copy what you need into ' + rule
    : '  left ' + old + ': it could not be moved aside; move your additions into ' + rule + ' and delete it');
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
