#!/usr/bin/env node
/* global process, require -- Node.js runtime */
// chisle: cross-platform installer.
//
// Detects the AI coding agents on this machine and installs Chisle for each:
//   - Claude Code  → plugin (marketplace add + install), fallback to standalone
//                    hooks + settings.json merge + statusline badge
//   - Pi           → Pi package (extension + skill)
//   - Gemini CLI   → gemini extensions install
//   - Codex        → fenced ruleset appended to ~/.codex/AGENTS.md
//   - Copilot CLI  → fenced ruleset + postToolUse compression hook in ~/.copilot
//   - OpenCode     → fenced ruleset + skills + plugin in its global config root
//   - Hermes       → skills copy in ~/.hermes/skills (Agent Skills standard)
//   - Cursor/Windsurf/Cline/Kiro/Copilot/Antigravity → project rule file dropped into CWD
//
// Usage:
//   npx chisle                 auto-detect + install
//   npx chisle --list          show detected agents, install nothing
//   npx chisle --only claude   install for one agent
//   npx chisle --dry-run       print actions, change nothing
//   npx chisle --uninstall     remove what we installed
//   npx chisle mcp wrap        route stdio MCP servers through the compressor
//   npx chisle --help
//
// Pure stdlib, zero runtime deps.

'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');

const MCP = require('./lib/mcp');
const TOOLS = require('./lib/tools');
const ENV = require('./lib/env');
const FENCE = require('./lib/fence');
const CLAUDE = require('./lib/claude');
const { INSTALLERS, installProjectRule } = require('./lib/agents');
const { uninstall, piSources } = require('./lib/uninstall');
const { readStats, askStar } = require('./lib/star');

const { die, expandHome } = ENV;

// ── Provider matrix ─────────────────────────────────────────────────────────
// scope: 'global' installs once for the whole machine/user.
//        'project' writes a per-repo rule file into the current directory
//        (that's how Cursor/Cline/Copilot rules actually work).
const PROVIDERS = [
  { id: 'claude',   label: 'Claude Code',   scope: 'global',  detect: 'cmd:claude' },
  { id: 'pi',       label: 'Pi',            scope: 'global',  detect: 'cmd:pi' },
  { id: 'gemini',   label: 'Gemini CLI',    scope: 'global',  detect: 'cmd:gemini' },
  { id: 'codex',    label: 'Codex CLI',     scope: 'global',  detect: 'cmd:codex||dir:~/.codex' },
  { id: 'copilot-cli', label: 'Copilot CLI', scope: 'global',  detect: 'cmd:copilot' },
  { id: 'opencode', label: 'OpenCode',        scope: 'global',  detect: 'cmd:opencode' },
  { id: 'hermes',   label: 'Hermes Agent',    scope: 'global',  detect: 'cmd:hermes||dir:~/.hermes' },
  { id: 'cursor',   label: 'Cursor',        scope: 'project', detect: 'cmd:cursor||dir:~/.cursor',
    rule: '.cursor/rules/chisle.mdc' },
  { id: 'windsurf', label: 'Windsurf',      scope: 'project', detect: 'cmd:windsurf||dir:~/.windsurf||dir:~/.codeium/windsurf',
    rule: '.windsurf/rules/chisle.md' },
  { id: 'cline',    label: 'Cline',         scope: 'project', detect: 'vscode-ext:cline',
    rule: '.clinerules/chisle.md' },
  { id: 'kiro',     label: 'Kiro',          scope: 'project', detect: 'cmd:kiro||dir:~/.kiro',
    rule: '.kiro/steering/chisle.md' },
  { id: 'copilot',  label: 'GitHub Copilot',scope: 'project', detect: 'vscode-ext:github.copilot||vscode-ext:github.copilot-chat',
    rule: '.github/copilot-instructions.md' },
  // Antigravity IDE (`antigravity`) and its CLI (`agy`, antigravity-cli) both
  // load workspace rules from .agents/rules/*.md.
  { id: 'antigravity', label: 'Antigravity', scope: 'project', detect: 'cmd:antigravity||cmd:agy||dir:~/.antigravity',
    rule: '.agents/rules/chisle.md' },
];

// ── argv ────────────────────────────────────────────────────────────────────
// Boolean flags. --update is --force narrowed to what is already there:
// refresh every agent that has Chisle, add it to none that don't.
const FLAGS = new Map([
  ['--dry-run', o => { o.dryRun = true; }],
  ['--force', o => { o.force = true; }],
  ['--list', o => { o.list = true; }],
  ['--stats', o => { o.stats = true; }],
  ['--update', o => { o.update = true; o.force = true; }],
  ['--uninstall', o => { o.uninstall = true; }],
  ['-u', o => { o.uninstall = true; }],
  ['--no-color', o => { o.noColor = true; }],
  ['-h', o => { o.help = true; }],
  ['--help', o => { o.help = true; }],
]);

// Flags that take a value: each stores the next argument.
const VALUE_FLAGS = new Map([
  ['--only', (opts, v) => {
    if (!v) die('error: --only requires an agent id');
    opts.only.push(v);
  }],
  ['--config-dir', (opts, v) => {
    if (!v || v.startsWith('--')) die('error: --config-dir requires a path');
    opts.configDir = expandHome(v);
  }],
]);

function parseArgs(argv) {
  const opts = { dryRun: false, force: false, list: false, uninstall: false,
                 noColor: false, help: false, stats: false, update: false, only: [], configDir: null };
  const args = argv.slice();
  while (args.length) {
    const a = args.shift();
    if (a === '--') continue; // npx sometimes forwards the literal end-of-options marker
    if (FLAGS.has(a)) { FLAGS.get(a)(opts); continue; }
    if (VALUE_FLAGS.has(a)) { VALUE_FLAGS.get(a)(opts, args.shift()); continue; }
    die(`error: unknown flag: ${a}\nrun 'npx chisle --help' for usage`);
  }
  const known = new Set(PROVIDERS.map(p => p.id));
  for (const id of opts.only) if (!known.has(id)) die(`error: unknown agent: ${id}\n  see 'npx chisle --list'`);
  return opts;
}

// ── color ───────────────────────────────────────────────────────────────────
function makeChalk(noColor) {
  const use = !noColor && process.stdout.isTTY && !process.env.NO_COLOR;
  const wrap = (c) => (s) => use ? `\x1b[${c}m${s}\x1b[0m` : s;
  return { orange: wrap('38;5;172'), dim: wrap('2'), red: wrap('31'), green: wrap('32'), yellow: wrap('33') };
}

// ── detection ───────────────────────────────────────────────────────────────
function vscodeExt(needle) {
  const lower = needle.toLowerCase();
  for (const r of ['.vscode/extensions', '.vscode-server/extensions', '.cursor/extensions', '.windsurf/extensions']) {
    const root = path.join(os.homedir(), r);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Fixed editor extension directories under the home directory.
    try { if (fs.readdirSync(root).some(e => e.toLowerCase().includes(lower))) return true; } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  }
  return false;
}

const DETECTORS = new Map([
  ['cmd', ENV.hasCmd],
  ['dir', ENV.isDir],
  ['file', ENV.isFile],
  ['vscode-ext', vscodeExt],
]);

function detectMatch(spec) {
  return spec.split('||').some(clause => {
    const c = clause.trim();
    const colon = c.indexOf(':');
    const test = DETECTORS.get(c.slice(0, colon));
    return !!test && test(expandHome(c.slice(colon + 1)));
  });
}

function providerDetected(provider) {
  return detectMatch(provider.detect) ||
    (provider.id === 'opencode' && ENV.isDir(ENV.opencodeDir()));
}

// ── what is already installed ───────────────────────────────────────────────
// Every install path skips when Chisle is already present, which is right for
// `npx chisle` and useless for an upgrade: the run reports success and changes
// nothing. --update pairs this check with --force so a refresh touches exactly
// the agents that already have it.
function hasFence(file) {
  try { return FENCE.exists(file) && FENCE.readText(file).includes(FENCE.FENCE_BEGIN); } catch { return false; }
}

const INSTALLED = new Map([
  ['claude', opts => CLAUDE.claudeInstalled(opts)],
  ['pi', () => ENV.hasCmd('pi') && piSources(ENV.capture('pi', ['list']).stdout).length > 0],
  ['gemini', () => ENV.hasCmd('gemini') && ENV.listsChisle('gemini', ['extensions', 'list'])],
  ['codex', () => hasFence(path.join(ENV.homeDir(), '.codex', 'AGENTS.md'))],
  ['opencode', () => hasFence(path.join(ENV.opencodeDir(), 'AGENTS.md'))
    || FENCE.exists(path.join(ENV.opencodeDir(), 'skills', 'chisle', 'SKILL.md'))],
  ['hermes', () => FENCE.exists(path.join(ENV.homeDir(), '.hermes', 'skills', 'chisle', 'SKILL.md'))],
  ['copilot-cli', () => FENCE.exists(path.join(ENV.copilotDir(), 'hooks', 'chisle.json'))],
]);

function installedFor(prov, opts) {
  try {
    if (INSTALLED.has(prov.id)) return INSTALLED.get(prov.id)(opts);
    return !!prov.rule && FENCE.exists(path.join(process.cwd(), prov.rule)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  } catch { return false; }
}

// ── help / banner ───────────────────────────────────────────────────────────
function printHelp(c) {
  process.stdout.write(`${c.orange('chisle')}: maximum-efficiency dev mode installer

Usage:
  npx chisle [flags]

Flags:
  --list           Detect agents and print them; install nothing
  --stats          Print tool-output savings recorded so far; change nothing
  --update         Refresh every agent that already has Chisle; install none
  --only <id>      Limit install/uninstall to one agent (repeatable)
  --dry-run        Print actions, change nothing
  --force          Reinstall / overwrite even if already present
  --uninstall, -u  Remove what chisle installed (also restores wrapped MCP servers)
  --config-dir <p> Override Claude config dir (default: $CLAUDE_CONFIG_DIR or ~/.claude)
  --no-color       Disable ANSI color
  --help, -h       This help

Agents: ${PROVIDERS.map(p => p.id).join(', ')}

MCP output compression (opt-in, any agent that launches stdio MCP servers):
  npx chisle mcp wrap   [--only <id>] [--file <mcp.json>] [--dry-run]
  npx chisle mcp unwrap [--only <id>] [--file <mcp.json>] [--dry-run]
  npx chisle mcp status
  Targets: ${[...new Set(MCP.targets().map(t => t.id))].join(', ')}. Never Claude Code
  (its own hook already compresses MCP output).

Tool schemas (Claude Code; every request carries every loaded tool's schema):
  npx chisle tools            # schema size vs. your own calls, last 30 days
  npx chisle tools trim       # deny the heavy tools you have not used [--days N] [--dry-run]
  npx chisle tools restore    # remove exactly what trim added

Examples:
  npx chisle                  # auto-detect + install
  npx chisle --only claude    # just Claude Code
  npx chisle --dry-run        # preview
  npx chisle --stats          # what the compressor has saved
  npx chisle mcp wrap         # compress MCP tool output in Cursor, Codex, Gemini, ...
  npx chisle@latest --update  # upgrade what is already installed
`);
}

// ── stats ───────────────────────────────────────────────────────────────────
// Reads the compressor's ledger. Input axis only, and deliberately so: chars
// elided have a real baseline (we know exactly what was cut), while the output
// axis has none, since there is no way to know what the model would have written
// without the ruleset, which is what the benchmark arms are for. A counter that
// blended the two would be inventing the interesting half.
function printStats(c, opts) {
  const only = opts.only.length === 1 ? opts.only[0] : null;
  const dir = only === 'opencode' ? ENV.opencodeStateDir()
    : only === 'copilot-cli' ? ENV.copilotDir()
    : ENV.claudeDir(opts);
  const p = path.join(dir, '.chisle-compress-stats.json'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  process.stdout.write(c.orange('chisle') + ': tool-output savings\n\n');
  const stats = readStats(dir);

  if (!stats || stats.events === 0) {
    process.stdout.write('  no compression recorded yet\n\n');
    process.stdout.write(c.dim(`  ledger: ${p}\n`));
    process.stdout.write(c.dim('  it fills as the compressor elides oversized tool output.\n'));
    process.stdout.write(c.dim("  nothing there after real use? check `npx chisle --list` and that Chisle is active.\n"));
    return;
  }

  const chars = stats.savedChars;
  const tokens = Math.floor(chars / 4);
  const per = Math.round(chars / stats.events);
  const fmt = (n) => n.toLocaleString('en-US');
  process.stdout.write(`  saved:      ${c.green(fmt(chars) + ' chars')}  (~${fmt(tokens)} tokens)\n`);
  process.stdout.write(`  outputs:    ${fmt(stats.events)} compressed, ${fmt(per)} chars each on average\n\n`);
  process.stdout.write(c.dim('  A floor, not an estimate: every saved byte also stops being re-sent\n'));
  process.stdout.write(c.dim('  on every later request in that session.\n\n'));
  process.stdout.write(c.dim(`  ledger: ${p}\n`));
  process.stdout.write(c.dim('  per-transcript detail: node benchmarks/replay-compress.js (in a repo clone)\n'));
}

function printList(c) {
  process.stdout.write(c.orange('chisle') + ': detected agents:\n\n');
  for (const p of PROVIDERS) {
    const found = providerDetected(p);
    const mark = found ? c.green('✓') : c.dim('·');
    const scope = p.scope === 'project' ? c.dim(' (project-scoped)') : '';
    process.stdout.write(`  ${mark} ${p.label}${scope}\n`);
  }
  process.stdout.write('\nRun ' + c.orange('npx chisle') + ' to install for the detected (✓) agents.\n');
}

// ── main ────────────────────────────────────────────────────────────────────
function runTools() {
  const args = process.argv.slice(3);
  const i = args.indexOf('--config-dir');
  const dir = i >= 0 ? expandHome(args.splice(i, 2)[1] || '') : ENV.claudeDir({});
  try { return TOOLS.main(args, dir); } catch (e) { return die('error: ' + e.message); }
}

function reportNothingToDo({ opts, warn, note }) {
  if (opts.update) {
    warn('Nothing to update: Chisle is not installed for any detected agent.');
    note('Run `npx chisle` to install it, or `npx chisle --list` to see what we look for.');
    note('Project-scoped agents (Cursor, Windsurf, Cline, Kiro, Copilot, Antigravity) are per-repo:');
    note('run this from the project that has the rule file.');
    return;
  }
  warn('No supported agents detected.');
  note('Run `npx chisle --list` to see what we look for, or `--only <id>` to force one.');
}

function installFor(ctx, p) {
  if (p.id === 'claude') CLAUDE.installClaude(ctx);
  else if (INSTALLERS.has(p.id)) INSTALLERS.get(p.id)(ctx);
  else installProjectRule(ctx, p);
}

function printSummary({ say, c, results }) {
  say(c.orange('── summary ──'));
  say(`  detected:  ${results.detected}`);
  if (results.installed.length) say(c.green(`  installed: ${results.installed.join(', ')}`));
  if (results.skipped.length)   say(c.dim(`  skipped:   ${results.skipped.map(s => s[0]).join(', ')}`));
  if (results.failed.length)    say(c.red(`  failed:    ${results.failed.map(s => s[0] + ' (' + s[1] + ')').join(', ')}`));
  say('');
}

function refreshMcpProxy({ note, warn }) {
  try {
    const dir = MCP.refreshProxy();
    if (dir) note(`  MCP proxy refreshed in ${dir}`);
  } catch (e) { warn(`  MCP proxy refresh failed: ${e.message}`); }
}

function install(ctx) {
  const { say, opts, results, c } = ctx;
  say(c.orange(opts.update ? '╭─ Chisle updater ─╮' : '╭─ Chisle installer ─╮'));
  say(c.dim(opts.dryRun ? '  (dry run, nothing will change)' : '  maximum signal, minimum noise'));
  say('');

  if (!opts.dryRun) refreshMcpProxy(ctx);

  let targets = PROVIDERS.filter(p => opts.only.length ? opts.only.includes(p.id) : providerDetected(p));
  if (opts.update) targets = targets.filter(p => installedFor(p, opts));
  if (targets.length === 0) return reportNothingToDo(ctx);

  for (const p of targets) installFor(ctx, p);
  printSummary(ctx);
  const code = results.failed.length ? 1 : 0;
  if (!opts.dryRun && results.installed.length) {
    say(c.orange('Done.') + ' Restart your agent. Type ' + c.orange('/chisle') + ' in Claude Code or Pi, or just start coding.');
    return askStar(ctx).catch(() => {}).then(() => process.exit(code));
  }
  return process.exit(code);
}

function main() {
  if (process.argv[2] === 'mcp') return MCP.main(process.argv.slice(3));
  if (process.argv[2] === 'tools') return runTools();
  const opts = parseArgs(process.argv.slice(2));
  const c = makeChalk(opts.noColor);

  if (opts.help) return printHelp(c);
  if (opts.list) return printList(c);
  if (opts.stats) return printStats(c, opts);

  const results = { detected: 0, installed: [], skipped: [], failed: [] };
  const say = (s) => process.stdout.write(s + '\n');
  const note = (s) => process.stdout.write(c.dim(s) + '\n');
  const warn = (s) => process.stdout.write(c.yellow(s) + '\n');
  const ctx = { say, note, warn, opts, results, c };

  if (opts.uninstall) return uninstall(ctx, PROVIDERS);
  return install(ctx);
}

main();
