#!/usr/bin/env node
/* global __dirname, process, require -- Node.js runtime */
'use strict';

// Runs benchmarks/quality/grade.js on hand-written code cells, so the bubblewrap
// sandbox it grades model answers in is exercised end to end. Linux only; the
// sandbox job sets GRADE_SANDBOX_REQUIRED=1 so a missing or broken bwrap fails.

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { exists, repoPath, tempRoot, under, writeJson, writeText } = require('./helpers/sandbox');

const GRADE = repoPath('benchmarks', 'quality', 'grade.js');
const REQUIRED = process.env.GRADE_SANDBOX_REQUIRED === '1';
const BWRAP = ['/usr/bin/bwrap', '/usr/local/bin/bwrap'].find(exists);

function unavailable() {
  if (process.platform !== 'linux') return `grade.js sandboxes code answers with bubblewrap, which needs Linux (this is ${process.platform})`;
  if (!BWRAP) return 'bubblewrap is not installed';
  return '';
}

const CORRECT = `function isBalanced(s) {
  const pairs = { ')': '(', ']': '[', '}': '{' };
  const stack = [];
  for (const c of s) {
    if ('([{'.includes(c)) stack.push(c);
    else if (c in pairs && stack.pop() !== pairs[c]) return false;
  }
  return stack.length === 0;
}`;

// Escapes vm through the host-realm context object and reports what the host
// process can see: a file outside the sandbox, the environment, and /usr as a
// control that proves the escape reached the real fs module.
const escape = (secret) => `const proc = this.constructor.constructor('return process')();
const fs = proc.mainModule.require('fs');
function look(file) { try { return fs.readFileSync(file, 'utf8'); } catch (e) { return 'unreadable: ' + e.code; } }
function isBalanced(s) {
  return { secret: look(${JSON.stringify(secret)}), repo: look(${JSON.stringify(GRADE)}).slice(0, 20), env: Object.keys(proc.env), usr: fs.existsSync('/usr/bin') };
}`;

// Correct answers, then a process that outlives them: it ignores SIGTERM and
// keeps an interval alive, so only the deadline's SIGKILL ends it.
const HANG = `${CORRECT}
const proc = this.constructor.constructor('return process')();
proc.on('SIGTERM', () => {});
this.constructor.constructor('return setInterval')()(() => {}, 1000);`;

const cell = (code) => ({ result: 'Here it is.\n\n```js\n' + code + '\n```\n', usage: { output_tokens: 1 } });

test('grade.js runs code answers inside the bubblewrap sandbox', { timeout: 120000 }, (t) => {
  const reason = unavailable();
  if (reason) {
    if (REQUIRED) assert.fail(`GRADE_SANDBOX_REQUIRED=1, but ${reason}`);
    t.skip(reason);
    return;
  }
  const raw = tempRoot(t, 'grade-sandbox-');
  const secret = under(raw, 'host-secret.txt');
  writeText(secret, 'host only');
  const cells = { 1: [CORRECT, CORRECT], 2: [escape(secret), CORRECT], 3: [HANG, CORRECT] };
  for (const [seed, [vanilla, chisle]] of Object.entries(cells)) {
    writeJson(under(raw, `code__balanced__vanilla__${seed}.json`), cell(vanilla));
    writeJson(under(raw, `code__balanced__chisle__${seed}.json`), cell(chisle));
  }

  // Without NODE_V8_COVERAGE, so grade.js stays out of the coverage report.
  const env = { ...process.env, RAW_DIR: raw };
  delete env.NODE_V8_COVERAGE;
  const started = Date.now();
  const r = spawnSync(process.execPath, [GRADE, '--json'], { env, encoding: 'utf8', timeout: 90000 });
  const seconds = (Date.now() - started) / 1000;
  if (r.status === 2) {
    if (REQUIRED) assert.fail(`GRADE_SANDBOX_REQUIRED=1, but grade.js could not sandbox: ${r.stderr.trim()}`);
    t.skip(`bubblewrap cannot create the sandbox here: ${r.stderr.trim()}`);
    return;
  }
  assert.equal(r.status, 0, r.stderr);
  const rows = JSON.parse(r.stdout).rows;
  const row = (arm, seed) => rows.find((x) => x.arm === arm && x.seed === seed);
  assert.equal(rows.length, 6);

  for (const arm of ['vanilla', 'chisle']) assert.deepEqual([row(arm, 1).pass, row(arm, 1).why], [true, ''], `${arm} seed 1`);
  for (const seed of [2, 3]) assert.equal(row('chisle', seed).pass, true);

  const seen = JSON.parse(row('vanilla', 2).why.match(/ -> (.*) want true$/)[1]);
  assert.equal(seen.usr, true, 'the escape reached the real fs module');
  assert.match(seen.secret, /^unreadable: /);
  assert.match(seen.repo, /^unreadable: /);
  assert.ok(seen.env.every((k) => k === 'PWD'), `environment: ${seen.env.join(', ')}`);

  assert.deepEqual([row('vanilla', 3).pass, row('vanilla', 3).why], [false, 'sandbox: SIGKILL']);
  // One 24 s deadline (3 s per case plus 2) and 5 quick cells.
  assert.ok(seconds < 60, `grading took ${seconds} s`);
});
