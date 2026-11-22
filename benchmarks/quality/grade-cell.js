#!/usr/bin/env node
/* global process, require -- Node.js runtime */
// Runs one model code answer against its hidden cases. grade.js starts this
// file under bubblewrap (no network, read-only system directories, an empty
// /tmp, no inherited environment), because vm alone is not a boundary for untrusted code.
// Reads {item, cleaned} as JSON on stdin and writes {pass, why} as JSON on stdout.

'use strict';

const vm = require('vm');

function gradeSource(item, cleaned) {
  const ctx = { module: { exports: {} }, exports: {}, console: { log() {} }, require: () => { throw new Error('no deps allowed'); } };
  vm.createContext(ctx);
  try {
    // Running the model's answer is this file's job; the bubblewrap sandbox is the boundary.
    // nosemgrep: rules_lgpl_javascript_eval_rule-vm-code-injection
    vm.runInContext(cleaned + `\n;globalThis.__fn = typeof ${item.fn} === 'function' ? ${item.fn} : undefined;`, ctx, { timeout: 3000 }); // NOSONAR
  } catch (e) { return { pass: false, why: 'eval: ' + e.message }; }
  const fn = ctx.__fn;
  if (typeof fn !== 'function') return { pass: false, why: `no function ${item.fn}` };
  for (const [args, want] of item.cases) {
    let got;
    // Build args inside the context: host-realm objects fail a correct
    // `Object.getPrototypeOf(x) === Object.prototype` plain-object check.
    try { got = vm.runInContext(`__fn(...JSON.parse(${JSON.stringify(JSON.stringify(args))}))`, ctx, { timeout: 3000 }); } // NOSONAR: calls the answer in its own context
    catch (e) { return { pass: false, why: `threw on ${JSON.stringify(args)}: ${e.message}` }; }
    if (JSON.stringify(got) !== JSON.stringify(want)) {
      return { pass: false, why: `${JSON.stringify(args)} -> ${JSON.stringify(got)} want ${JSON.stringify(want)}` };
    }
  }
  return { pass: true, why: '' };
}

let input = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (c) => { input += c; });
process.stdin.on('end', () => {
  const { item, cleaned } = JSON.parse(input);
  process.stdout.write(JSON.stringify(gradeSource(item, cleaned)));
});
