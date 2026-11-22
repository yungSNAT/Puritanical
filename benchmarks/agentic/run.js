#!/usr/bin/env node
/* global __dirname, console, process, require -- Node.js runtime */
// Agentic benchmark: the arms get a real repo and real tools, not a chat prompt.
//
// Each fixture is a small repo with a planted trap that only a careful agent
// avoids -- an existing helper that should be reused, a shared bug with three
// call sites, a one-line config ask that invites a framework. The agent never
// sees the test: test.js is copied in AFTER the run and decides pass/fail.
//
// Usage: node benchmarks/agentic/run.js [model] [seeds] [parallel]
//   ARMS=vanilla,chisle,chisle-hook,chisle-trim,ponytail,rtk,rtk-chisle-hook  arms to run (default: vanilla,chisle)
//   rtk = bare agent plus RTK's PreToolUse rewrite hook (`rtk hook claude`);
//   rtk-chisle-hook = chisle-hook plus that same rtk hook
//   chisle-trim = chisle plus the deny list `npx chisle tools trim` writes for
//   tools a coding loop never calls (TRIM_TOOLS overrides the list)
//   PONYTAIL_SKILL=path              ponytail SKILL.md (default: installed plugin cache)
//   CHISLE_SKILL=path                candidate ruleset for the chisle arms (default: skills/puritanical/SKILL.md)
//   FIXTURES=noisylog,reuse          run only these fixtures (default: all)
//   RAW_DIR=path                     where cells go (default: ./raw); use a fresh
//                                    dir when the ruleset changes, or old cells
//                                    are reused as if they were new
// Resumable: a cell whose result JSON already exists is skipped.

const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile, execFileSync } = require('child_process');
const crypto = require('crypto');

const MODEL = process.argv[2] || 'claude-haiku-4-5-20251001';
const SEEDS = Number(process.argv[3] || 3);
const PAR = Number(process.argv[4] || 5);

const KNOWN_ARMS = ['vanilla', 'chisle', 'chisle-hook', 'chisle-trim', 'ponytail', 'rtk', 'rtk-chisle-hook'];
const ARMS = (process.env.ARMS || 'vanilla,chisle').split(',').map((a) => a.trim()).filter(Boolean);
const unknown = ARMS.filter((a) => !KNOWN_ARMS.includes(a));
if (unknown.length) { console.error(`unknown arm(s): ${unknown.join(', ')}; known: ${KNOWN_ARMS.join(', ')}`); process.exit(1); }

const HERE = __dirname;
const ROOT = path.join(HERE, '..', '..');
const FIX = path.join(HERE, 'fixtures');
const RAW = process.env.RAW_DIR ? path.resolve(process.env.RAW_DIR) : path.join(HERE, 'raw');
// eslint-disable-next-line security/detect-non-literal-fs-filename -- RAW_DIR from the environment or raw/ beside this script.
fs.mkdirSync(RAW, { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

const ISO = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-agentic-'));
fs.copyFileSync(path.join(os.homedir(), '.claude', '.credentials.json'), path.join(ISO, '.credentials.json'));
// eslint-disable-next-line security/detect-non-literal-fs-filename -- CHISLE_SKILL from the environment or this repo's skills/puritanical/SKILL.md.
const skill = fs.readFileSync(process.env.CHISLE_SKILL || path.join(HERE, '..', '..', 'skills', 'puritanical', 'SKILL.md'), 'utf8'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
const body = skill.split(/^---\s*$/m).slice(2).join('---').trim();
if (!body) { console.error('empty chisle system prompt'); process.exit(1); }
// eslint-disable-next-line security/detect-non-literal-fs-filename -- The prompt file in the private temporary config directory.
fs.writeFileSync(path.join(ISO, 'chisle.txt'), body + '\n'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
// ponytail arm (#37): the rival's skill body, appended the same way.
if (ARMS.includes('ponytail')) {
  const cache = path.join(os.homedir(), '.claude', 'plugins', 'cache', 'ponytail', 'ponytail');
  let src = process.env.PONYTAIL_SKILL;
  if (!src) {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Version directories of the installed plugin cache under the home directory.
    try { src = fs.readdirSync(cache).map((v) => path.join(cache, v, 'skills', 'ponytail', 'SKILL.md')).find((f) => fs.existsSync(f)); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  }
  if (!src) { console.error('ponytail SKILL.md not found; set PONYTAIL_SKILL'); process.exit(1); }
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- PONYTAIL_SKILL from the environment or the plugin cache found above.
  const pt = fs.readFileSync(src, 'utf8').split(/^---\s*$/m).slice(2).join('---').trim(); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  if (!pt) { console.error('empty ponytail system prompt'); process.exit(1); }
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- The prompt file in the private temporary config directory.
  fs.writeFileSync(path.join(ISO, 'ponytail.txt'), pt + '\n'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  console.log('ponytail skill: ' + src);
}
process.on('exit', () => fs.rmSync(ISO, { recursive: true, force: true }));

// chisle-hook arm: the ruleset plus the shipped PreToolUse wrapper and
// PostToolUse compressor, taken verbatim from plugin.json so the arm cannot
// drift from what users install.
const plugin = JSON.parse(fs.readFileSync(path.join(ROOT, '.claude-plugin', 'plugin.json'), 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
const TRIM_SETTINGS = JSON.stringify({ permissions: { deny: (process.env.TRIM_TOOLS || 'Workflow,ScheduleWakeup,ReportFindings,ShareOnboardingGuide,SendFeedback').split(',') } });
const HOOK_SETTINGS = JSON.stringify({ hooks: { PreToolUse: plugin.hooks.PreToolUse, PostToolUse: plugin.hooks.PostToolUse } })
  .split('${CLAUDE_PLUGIN_ROOT}').join(ROOT);
// RTK arms: the hook `rtk init -g` installs, so the arm matches a real RTK user.
const RTK_HOOK = [{ matcher: 'Bash', hooks: [{ type: 'command', command: 'rtk hook claude' }] }];
const RTK_SETTINGS = JSON.stringify({ hooks: { PreToolUse: RTK_HOOK } });
const RTK_HOOK_SETTINGS = JSON.stringify({ hooks: { PreToolUse: RTK_HOOK.concat(plugin.hooks.PreToolUse), PostToolUse: plugin.hooks.PostToolUse } })
  .split('${CLAUDE_PLUGIN_ROOT}').join(ROOT);
if (ARMS.some((a) => a.startsWith('rtk'))) {
  try { console.log('rtk: ' + execFileSync('rtk', ['--version'], { encoding: 'utf8' }).trim()); } // NOSONAR: rtk from PATH, as the benchmark measures the user's installed rtk.
  catch { console.error('rtk arms need rtk on PATH'); process.exit(1); }
}

const only = process.env.FIXTURES ? process.env.FIXTURES.split(',').map((f) => f.trim()) : null;
// eslint-disable-next-line security/detect-non-literal-fs-filename -- The fixtures/ directory beside this script and its subdirectories.
const fixtures = fs.readdirSync(FIX).filter((f) => fs.existsSync(path.join(FIX, f, 'task.txt')) && (!only || only.includes(f))); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
if (only && fixtures.length !== only.length) { console.error(`unknown fixture in FIXTURES=${process.env.FIXTURES}`); process.exit(1); }

// eslint-disable-next-line security/detect-non-literal-fs-filename -- A cell's temporary copy of a fixture repo and its subdirectories.
const walk = (dir, base = dir) => fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => { // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  if (e.name === 'node_modules' || e.name === '.git') return [];
  const p = path.join(dir, e.name); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return e.isDirectory() ? walk(p, base) : [path.relative(base, p)];
});
const stat = (p) => {
  // Hash the contents: a one-character fix changes no line count, and comparing
  // line counts alone silently reported those runs as "no files touched".
  let src = '';
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- A file walk() found in a cell's temporary repo copy.
  try { src = fs.readFileSync(p, 'utf8'); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  return { loc: src.split('\n').filter((l) => l.trim()).length, hash: crypto.createHash('sha256').update(src).digest('hex') };
};
const snapshot = (root) => new Map(walk(root).map((f) => [f, stat(path.join(root, f))])); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal

const cells = [];
for (const arm of ARMS) {
  for (let seed = 1; seed <= SEEDS; seed++) {
    for (const fx of fixtures) cells.push({ fx, arm, seed });
  }
}
// One short line per tool call: enough to see the pattern, small enough to commit.
const summarize = (i) => String(i.command || i.pattern || (i.file_path ? path.basename(i.file_path)
  + (i.offset || i.limit ? ` [${i.offset || 1}+${i.limit || ''}]` : '') : '') || i.path || '').slice(0, 100);
const outPath = (c) => path.join(RAW, `${c.fx}__${c.arm}__${c.seed}.json`); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
// eslint-disable-next-line security/detect-non-literal-fs-filename -- A cell file under RAW, named from the arm list and the fixtures/ directory.
const todo = cells.filter((c) => !fs.existsSync(outPath(c))); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
let limited = false;
console.log(`model ${MODEL} | seeds ${SEEDS} | parallel ${PAR} | ${todo.length} of ${cells.length} cells to run`);

// Pairs each tool_result block with the tool_use call before it.
function recordBlock(calls, b) {
  if (b.type === 'tool_use') calls.push({ tool: b.name, input: summarize(b.input || {}) });
  if (b.type === 'tool_result') {
    const text = typeof b.content === 'string' ? b.content : (b.content || []).map((x) => x.text || '').join('');
    const call = calls[calls.length - 1];
    if (call && call.chars === undefined) { call.chars = text.length; if (b.is_error) call.error = true; }
  }
}

// The result event, the tool calls and each API request's prompt size from a stream-json transcript.
function parseStream(stdout) {
  let d = {};
  const calls = [];
  const prompts = new Map(); // API request id -> its prompt size in tokens
  for (const line of stdout.split('\n')) {
    let e; try { e = JSON.parse(line); } catch { continue; }
    if (e.type === 'result') d = e;
    const u = e.type === 'assistant' && e.message && e.message.usage;
    if (u) prompts.set(e.message.id, (u.input_tokens || 0) + (u.cache_creation_input_tokens || 0) + (u.cache_read_input_tokens || 0));
    const blocks = (e.message && Array.isArray(e.message.content)) ? e.message.content : [];
    for (const b of blocks) recordBlock(calls, b);
  }
  return { d, calls, prompts };
}

function recordRun(rec, stdout) {
  const { d, calls, prompts } = parseStream(stdout);
  rec.out_tokens = d.usage ? d.usage.output_tokens : 0;
  rec.input_tokens = d.usage ? (d.usage.input_tokens || 0) + (d.usage.cache_creation_input_tokens || 0) + (d.usage.cache_read_input_tokens || 0) : 0;
  rec.cost_usd = d.total_cost_usd || 0;
  rec.turns = d.num_turns || 0;
  rec.answer = (d.result || '').trim();
  rec.tool_calls = calls;
  rec.request_prompts = [...prompts.values()];
}

function recordCompression(rec, cfg) {
  let stats = {};
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- The hook's stats file in the cell's temporary config directory.
  try { stats = JSON.parse(fs.readFileSync(path.join(cfg, '.puritanical-compress-stats.json'), 'utf8')); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  rec.compressed_chars = stats.savedChars || 0;
  rec.compress_events = stats.events || 0;
}

// A usage-limit refusal is not a measurement. Writing it would score as
// "the agent changed nothing", which is exactly how a limit hit
// mid-suite produced 89 fake failures the first time this was run.
// Only a short, error-shaped result is a limit hit: matching "rate limit"
// anywhere also dropped valid answers that merely mention rate limiting.
const limitHit = (rec) => !rec.out_tokens
  || (rec.answer.length < 300 && /limit reached|hit your .{0,20}limit|usage limit|session limit|rate_limit_error/i.test(rec.answer));

// The hidden test lands only now, after the agent is finished.
function runHiddenTest(c, repo, rec) {
  const hidden = path.join(repo, '__hidden_test.js'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  fs.copyFileSync(path.join(FIX, c.fx, 'test.js'), hidden); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal, rules_lgpl_javascript_traversal_rule-join-resolve-path-traversal
  try {
    execFileSync(process.execPath, ['__hidden_test.js'], { cwd: repo, timeout: 20000, stdio: 'pipe' });
    rec.pass = true; rec.test_output = '';
  } catch (e) {
    rec.pass = false;
    rec.test_output = String((e.stdout || '') + (e.stderr || '')).trim().slice(0, 400);
  }
  fs.rmSync(hidden, { force: true });
}

function recordChanges(rec, repo, before) {
  const after = snapshot(repo);
  rec.files_created = [...after.keys()].filter((f) => !before.has(f));
  rec.files_touched = [...after].filter(([f, s]) => before.has(f) && s.hash !== before.get(f).hash).map(([f]) => f);
  rec.loc_before = [...before.values()].reduce((a, b) => a + b.loc, 0);
  rec.loc_after = [...after.values()].reduce((a, b) => a + b.loc, 0);
  rec.loc_delta = rec.loc_after - rec.loc_before;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- package.json in the cell's temporary repo copy.
    const pkg = JSON.parse(fs.readFileSync(path.join(repo, 'package.json'), 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    rec.deps_added = Object.keys({ ...(pkg.dependencies || {}), ...(pkg.devDependencies || {}) });
  } catch { rec.deps_added = []; }
}

function runCell(c) {
  return new Promise((resolve) => {
    const work = fs.mkdtempSync(path.join(os.tmpdir(), `wk-${c.fx}-`)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    const repo = path.join(work, 'repo'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    fs.cpSync(path.join(FIX, c.fx, 'repo'), repo, { recursive: true }); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    const before = snapshot(repo);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- A fixture's task file under fixtures/ beside this script.
    const task = fs.readFileSync(path.join(FIX, c.fx, 'task.txt'), 'utf8').trim(); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal

    // stream-json, so each cell also records the tool calls it made, not only totals.
    const args = ['-p', task, '--model', MODEL, '--output-format', 'stream-json', '--verbose', '--dangerously-skip-permissions'];
    const hooked = c.arm === 'chisle-hook' || c.arm === 'rtk-chisle-hook';
    if (c.arm !== 'vanilla' && c.arm !== 'rtk') args.push('--append-system-prompt-file', path.join(ISO, c.arm === 'ponytail' ? 'ponytail.txt' : 'chisle.txt'));
    if (c.arm === 'chisle-trim') args.push('--settings', TRIM_SETTINGS);
    let cfg = ISO;
    if (c.arm === 'rtk') args.push('--settings', RTK_SETTINGS);
    if (hooked) {
      // Own config dir per cell: the hook keeps its on/off flag, savings ledger
      // and spill files there, and parallel cells must not share them.
      cfg = path.join(work, 'cfg'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- The cell's config directory inside its temporary work directory.
      fs.mkdirSync(cfg); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      fs.copyFileSync(path.join(ISO, '.credentials.json'), path.join(cfg, '.credentials.json')); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- The hook's on flag in the cell's temporary config directory.
      fs.writeFileSync(path.join(cfg, '.puritanical-active'), 'on'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      args.push('--settings', c.arm === 'rtk-chisle-hook' ? RTK_HOOK_SETTINGS : HOOK_SETTINGS);
    }

    execFile('claude', args, { // NOSONAR: claude from PATH, as the benchmark measures the user's installed CLI.
      cwd: repo,
      timeout: 300000,
      maxBuffer: 32 * 1024 * 1024,
      env: { ...process.env, HOME: work, CLAUDE_CONFIG_DIR: cfg },
    }, (err, stdout) => {
      const rec = { fixture: c.fx, arm: c.arm, seed: c.seed };
      if (err && !stdout) {
        rec.error = String(err.message).slice(0, 200);
      } else {
        recordRun(rec, stdout);
        if (hooked) recordCompression(rec, cfg);
        if (limitHit(rec)) {
          fs.rmSync(work, { recursive: true, force: true });
          console.log(`  ${c.fx}/${c.arm}/${c.seed} LIMITED — not recorded`);
          limited = true;
          resolve();
          return;
        }
        runHiddenTest(c, repo, rec);
        recordChanges(rec, repo, before);
      }
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- A cell file under RAW, named from the arm list and the fixtures/ directory.
      fs.writeFileSync(outPath(c), JSON.stringify(rec, null, 1)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      fs.rmSync(work, { recursive: true, force: true });
      console.log(`  ${c.fx}/${c.arm}/${c.seed} ${rec.error ? 'ERROR' : rec.pass ? 'PASS' : 'FAIL'}`);
      resolve();
    });
  });
}

void (async () => {
  const queue = todo.slice();
  await Promise.all(Array.from({ length: PAR }, async () => {
    while (queue.length && !limited) await runCell(queue.shift());
  }));
  if (limited) console.log('\nSTOPPED: usage limit reached. No partial cells were written; re-run to resume.');
  console.log('done -> ' + RAW);
})();
