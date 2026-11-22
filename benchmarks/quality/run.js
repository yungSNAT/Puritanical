#!/usr/bin/env node
/* global __dirname, console, process, require -- Node.js runtime */
// Quality benchmark: does the Chisle ruleset change how OFTEN the model is RIGHT?
//
// Two arms, identical prompt, identical model. vanilla appends nothing; chisle
// appends skills/chisle/SKILL.md (frontmatter stripped) as the system prompt --
// exactly what the shipped skill does. Every item is auto-graded:
//   math -> exact match on a required "ANSWER:" line
//   code -> hidden unit tests the model never sees
//
// Usage: node benchmarks/quality/run.js [model] [seeds] [parallel]
// Resumable: a cell whose raw JSON already exists is skipped, so point
// RAW_DIR at a fresh directory when the ruleset changes (grade.js reads it too).

const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');

const MODEL = process.argv[2] || 'claude-haiku-4-5-20251001';
const SEEDS = Number(process.argv[3] || 3);
const PAR = Number(process.argv[4] || 8);

const HERE = __dirname;
const RAW = process.env.RAW_DIR ? path.resolve(process.env.RAW_DIR) : path.join(HERE, 'raw');
// eslint-disable-next-line security/detect-non-literal-fs-filename -- RAW_DIR from the environment or raw/ beside this script.
fs.mkdirSync(RAW, { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

// Isolated config: credentials only, so no personal settings/CLAUDE.md/plugins leak in.
const ISO = fs.mkdtempSync(path.join(os.tmpdir(), 'chisle-bench-'));
fs.copyFileSync(path.join(os.homedir(), '.claude', '.credentials.json'), path.join(ISO, '.credentials.json'));
const skill = fs.readFileSync(path.join(HERE, '..', '..', 'skills', 'chisle', 'SKILL.md'), 'utf8'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
const body = skill.split(/^---\s*$/m).slice(2).join('---').trim();
if (!body) { console.error('empty chisle system prompt'); process.exit(1); }
// eslint-disable-next-line security/detect-non-literal-fs-filename -- The prompt file in the private temporary config directory.
fs.writeFileSync(path.join(ISO, 'chisle.txt'), body + '\n'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
process.on('exit', () => fs.rmSync(ISO, { recursive: true, force: true }));

// Response-format lines are appended here so both arms get byte-identical prompts.
const MATH_FMT = ' End your reply with a final line of exactly: ANSWER: <value>';
const CODE_FMT = ' Reply with exactly one JavaScript code block containing the function, and no other code block.';

const math = JSON.parse(fs.readFileSync(path.join(HERE, 'math.json'), 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
const code = JSON.parse(fs.readFileSync(path.join(HERE, 'code.json'), 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

const cells = [];
for (const arm of ['vanilla', 'chisle']) {
  for (let seed = 1; seed <= SEEDS; seed++) {
    for (const m of math) cells.push({ suite: 'math', id: m.id, arm, seed, prompt: m.prompt + MATH_FMT });
    for (const c of code) cells.push({ suite: 'code', id: c.id, arm, seed, prompt: c.prompt + CODE_FMT });
  }
}

// eslint-disable-next-line security/detect-non-literal-fs-filename -- A cell file under RAW, named from the item list in this directory.
const todo = cells.filter((c) => !fs.existsSync(outPath(c))); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
function outPath(c) { return path.join(RAW, `${c.suite}__${c.id}__${c.arm}__${c.seed}.json`); } // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal

console.log(`model ${MODEL} | seeds ${SEEDS} | parallel ${PAR} | ${todo.length} of ${cells.length} cells to run`);

let done = 0, failed = 0, limited = false;
function runCell(c) {
  return new Promise((resolve) => {
    const args = ['-p', c.prompt, '--model', MODEL, '--output-format', 'json'];
    if (c.arm === 'chisle') args.push('--append-system-prompt-file', path.join(ISO, 'chisle.txt'));
    execFile('claude', args, { // NOSONAR: claude from PATH, as the benchmark measures the user's installed CLI.
      cwd: os.tmpdir(),
      timeout: 180000,
      maxBuffer: 32 * 1024 * 1024,
      env: { ...process.env, HOME: os.tmpdir(), CLAUDE_CONFIG_DIR: ISO },
    }, (err, stdout) => {
      if (err || !stdout) { failed++; console.log(`  fail ${c.suite}/${c.id}/${c.arm}/${c.seed}`); resolve(); return; }

      // A usage-limit refusal is not a measurement -- it grades as a wrong
      // answer and silently poisons every later cell. Drop it and stop.
      let parsed = {};
      try { parsed = JSON.parse(stdout); } catch {}
      const text = String(parsed.result || '');
      // Only a short, error-shaped result is a limit hit: matching "rate limit"
      // anywhere also dropped valid answers that merely mention rate limiting.
      if (!(parsed.usage && parsed.usage.output_tokens) || (text.length < 300 && /limit reached|hit your .{0,20}limit|usage limit|session limit|rate_limit_error/i.test(text))) {
        limited = true;
        console.log(`  ${c.suite}/${c.id}/${c.arm}/${c.seed} LIMITED — not recorded`);
        resolve();
        return;
      }

      // eslint-disable-next-line security/detect-non-literal-fs-filename -- A cell file under RAW, named from the item list in this directory.
      fs.writeFileSync(outPath(c), stdout); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      if (++done % 10 === 0) console.log(`  ${done}/${todo.length}`);
      resolve();
    });
  });
}

void (async () => {
  const queue = todo.slice();
  const workers = Array.from({ length: PAR }, async () => {
    while (queue.length && !limited) await runCell(queue.shift());
  });
  await Promise.all(workers);
  console.log(`done. ${done} written, ${failed} failed -> ${RAW}`);
  if (limited) console.log('STOPPED: usage limit reached. No partial cells were written; re-run to resume.');
})();
