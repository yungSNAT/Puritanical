/* global module, require -- Node.js runtime */
// chisle installer: `--uninstall`. Each agent step removes only what Chisle
// installed under that agent's config root and returns the items it cleaned.

'use strict';

const fs = require('fs');
const path = require('path');
const SETTINGS = require('./settings');
const MCP = require('./mcp');
const TOOLS = require('./tools');
const ENV = require('./env');
const FENCE = require('./fence');

// Removes each existing fixed entry under root; returns the count.
function removeEntries(ctx, root, names, remove = FENCE.removeTree) {
  let n = 0;
  for (const name of names) {
    const p = path.join(root, name); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    if (!FENCE.exists(p)) continue;
    if (!ctx.opts.dryRun) remove(p);
    ctx.note('  removed ' + p);
    n++;
  }
  return n;
}

function unlinkQuietly(p) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Fixed Chisle state files in the Claude config directory.
  try { fs.unlinkSync(p); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function uninstallClaudePlugin(ctx) {
  if (!ENV.hasCmd('claude') || ctx.opts.dryRun || !ENV.listsChisle('claude', ['plugin', 'list'])) return 0;
  ENV.run('claude', ['plugin', 'uninstall', 'chisle@chisle'], ctx.opts.dryRun);
  return 1;
}

function isChisleStatusLine(sl) {
  return !!sl && typeof sl.command === 'string' && sl.command.includes('chisle-statusline');
}

// Standalone hooks + statusline in settings.json.
function uninstallClaudeSettings(ctx, cfg) {
  const settingsPath = path.join(cfg, 'settings.json'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const settings = SETTINGS.readSettings(settingsPath);
  if (!settings) return 0;
  const removed = SETTINGS.removeHooks(settings, 'chisle-');
  const slRemoved = isChisleStatusLine(settings.statusLine);
  if (slRemoved) delete settings.statusLine;
  if (removed === 0 && !slRemoved) return 0;
  if (!ctx.opts.dryRun) { SETTINGS.validateHookFields(settings); SETTINGS.writeSettings(settingsPath, settings); }
  ctx.note(`  removed ${removed} hook entr${removed === 1 ? 'y' : 'ies'}${slRemoved ? ' + statusline' : ''} from settings.json`);
  return 1;
}

// Tool schemas denied by `npx chisle tools trim`: remove exactly those entries.
function uninstallClaudeTools(ctx, cfg) {
  try {
    const restored = TOOLS.restore(cfg, { dryRun: ctx.opts.dryRun });
    if (!restored.length) return 0;
    ctx.note(`  removed tool deny entries added by chisle: ${restored.join(', ')}`);
    return 1;
  } catch (e) { ctx.warn('  ' + e.message); return 0; }
}

function uninstallClaude(ctx) {
  const cfg = ENV.claudeDir(ctx.opts);
  return uninstallClaudePlugin(ctx)
    + uninstallClaudeSettings(ctx, cfg)
    + uninstallClaudeTools(ctx, cfg)
    + removeEntries(ctx, cfg, ['chisle-hooks', 'chisle-spill'])
    + removeEntries(ctx, cfg, ['.chisle-active', '.chisle-session-turns', '.chisle-statusline-suffix',
      '.chisle-compress-stats.json', '.chisle-compress-last.json', '.chisle-update-check.json'], unlinkQuietly);
}

function uninstallPi(ctx) {
  if (!ENV.hasCmd('pi')) return 0;
  let n = 0;
  for (const source of piSources(ENV.capture('pi', ['list']).stdout)) {
    if (ENV.ranOk(ENV.run('pi', ['remove', source], ctx.opts.dryRun))) n++;
  }
  return n;
}

// Codex keeps its original removal: the block and its surrounding newlines
// become one newline.
function uninstallCodex(ctx) {
  const codexMd = path.join(ENV.homeDir(), '.codex', 'AGENTS.md');
  if (!FENCE.exists(codexMd)) return 0;
  const txt = FENCE.readText(codexMd);
  const { b, e } = FENCE.fenceSpan(txt);
  if (b === -1) return 0;
  let stripped = txt;
  if (e !== -1) {
    const start = b > 0 && txt.charAt(b - 1) === '\n' ? b - 1 : b;
    let end = e + FENCE.FENCE_END.length;
    if (txt.charAt(end) === '\n') end++;
    stripped = txt.slice(0, start) + '\n' + txt.slice(end);
  }
  if (!ctx.opts.dryRun) FENCE.writeText(codexMd, stripped.replace(/\n{3,}/g, '\n\n'));
  ctx.note(`  removed chisle block from ${codexMd}`);
  return 1;
}

// Removes the fenced block from an instructions file; trimEnd collapses a
// trailing blank run to one newline (Copilot CLI).
function removeFencedBlock(ctx, file, { warnUnclosed, trimEnd }) {
  if (!FENCE.exists(file)) return 0;
  const txt = FENCE.readText(file);
  const { b, e } = FENCE.fenceSpan(txt);
  if (b === -1) return 0;
  if (e === -1) {
    if (warnUnclosed) ctx.note('  ' + file + ' has a chisle-begin marker with no chisle-end; leaving it alone');
    return 0;
  }
  let stripped = FENCE.withoutFence(txt);
  if (trimEnd) stripped = stripped.replace(/\n{2,}$/, '\n');
  if (!ctx.opts.dryRun) FENCE.writeText(file, stripped);
  ctx.note('  removed chisle block from ' + file);
  return 1;
}

function uninstallSkills(ctx, skillsDir) {
  const gone = FENCE.pruneSkills(skillsDir, ctx.opts.dryRun);
  if (gone === 0) return 0;
  ctx.note('  removed ' + gone + ' chisle skill dir(s) from ' + skillsDir);
  return 1;
}

function uninstallOpencode(ctx) {
  const root = ENV.opencodeDir();
  return removeFencedBlock(ctx, path.join(root, 'AGENTS.md'), { warnUnclosed: true, trimEnd: false })
    + uninstallSkills(ctx, path.join(root, 'skills'))
    + removeEntries(ctx, path.join(root, 'plugins'), ['chisle.js', 'chisle-hooks', 'chisle'])
    + removeEntries(ctx, ENV.opencodeStateDir(),
      ['chisle-spill', 'chisle-dedup', '.chisle-compress-stats.json', '.chisle-compress-last.json']);
}

function uninstallHermes(ctx) {
  return uninstallSkills(ctx, path.join(ENV.homeDir(), '.hermes', 'skills'));
}

function uninstallCopilotCli(ctx) {
  const root = ENV.copilotDir();
  return removeFencedBlock(ctx, path.join(root, 'copilot-instructions.md'), { warnUnclosed: false, trimEnd: true })
    + removeEntries(ctx, root, [path.join('hooks', 'chisle.json'), 'chisle-hooks', 'chisle-spill', 'chisle-dedup',
      '.chisle-compress-stats.json']);
}

// MCP servers routed through the proxy by `npx chisle mcp wrap`: restore
// their original commands before anything else could remove the proxy.
function uninstallMcp(ctx) {
  let n = 0;
  const r = MCP.run(['unwrap'].concat(ctx.opts.dryRun ? ['--dry-run'] : []), () => {});
  if (r.changed) { ctx.note('  restored ' + r.changed + ' MCP server command(s) wrapped by `chisle mcp wrap`'); n++; }
  const pd = MCP.proxyDir();
  if (FENCE.exists(pd)) { if (!ctx.opts.dryRun) FENCE.removeTree(pd); ctx.note('  removed ' + pd); n++; }
  return n;
}

const STEPS = [
  ['claude', uninstallClaude],
  ['pi', uninstallPi],
  ['codex', uninstallCodex],
  ['opencode', uninstallOpencode],
  ['hermes', uninstallHermes],
  ['copilot-cli', uninstallCopilotCli],
];

function uninstall(ctx, providers) {
  const { say, note, opts, c } = ctx;
  say(c.orange('Chisle uninstall'));
  const wants = id => !opts.only.length || opts.only.includes(id);
  let touched = 0;
  for (const [id, step] of STEPS) if (wants(id)) touched += step(ctx);
  if (!opts.only.length) touched += uninstallMcp(ctx);

  const projectScoped = new Set(providers.filter(p => p.scope === 'project').map(p => p.id));
  if (!opts.only.length || opts.only.some(id => projectScoped.has(id))) {
    note('');
    note('Project-scoped rule files (.cursor/, .windsurf/, .clinerules/, .kiro/, .agents/rules/, .github/copilot-instructions.md)');
    note('live in your project repos, so remove them per-project with git if you added them there.');
  }
  say(touched ? c.green(`\nUninstalled. ${touched} item(s) cleaned.`) : c.yellow('\nNothing to uninstall.'));
}

function piSources(output) {
  // `pi list` may print more after the source (`npm:chisle@3.0.0 (user)`).
  return String(output || '').split('\n').map(line => line.trim().split(/\s+/)[0])
    .filter(line => /^npm:chisle(?:@|$)/i.test(line) || /^git:(?:https:\/\/|http:\/\/|git\+https:\/\/|ssh:\/\/git@|git@)?github\.com[/:](?:JayPokale\/Chisle|yungSNAT\/Puritanical)(?:@|$)/i.test(line))
    // The source is passed to `pi remove` through cmd.exe on Windows: refuse
    // anything beyond package-spec characters.
    .filter(line => /^[\w.:@/~+-]+$/.test(line))
    .map(source => source.replace(/@[^/@]+$/, ''));
}

module.exports = { uninstall, piSources };
