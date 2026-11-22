#!/usr/bin/env node
/* global process, require -- Node.js runtime */
// Runs one model code answer on its cases' arguments. grade.js starts this
// file under bubblewrap (no network, read-only system directories, an empty
// /tmp, no inherited environment), because vm alone is not a boundary for untrusted code.
// Reads {fn, args, cleaned} as JSON on stdin and writes the outputs as JSON on stdout:
// {why} when the answer does not load, else {got, threw}. The expected values never
// enter the sandbox, so an answer that escapes vm can forge outputs but not a verdict.

'use strict';

const vm = require('vm');

function run(fn, args, cleaned) {
  const ctx = { module: { exports: {} }, exports: {}, console: { log() {} }, require: () => { throw new Error('no deps allowed'); } };
  vm.createContext(ctx);
  try {
    // Running the model's answer is this file's job; the bubblewrap sandbox is the boundary.
    // nosemgrep: rules_lgpl_javascript_eval_rule-vm-code-injection
    vm.runInContext(cleaned + `\n;globalThis.__fn = typeof ${fn} === 'function' ? ${fn} : undefined;`, ctx, { timeout: 3000 }); // NOSONAR
  } catch (e) { return { why: 'eval: ' + e.message }; }
  if (typeof ctx.__fn !== 'function') return { why: `no function ${fn}` };
  const got = [];
  for (const a of args) {
    // Build args inside the context: host-realm objects fail a correct
    // `Object.getPrototypeOf(x) === Object.prototype` plain-object check.
    try { got.push(String(JSON.stringify(vm.runInContext(`__fn(...JSON.parse(${JSON.stringify(JSON.stringify(a))}))`, ctx, { timeout: 3000 })))); } // NOSONAR: calls the answer in its own context
    catch (e) { return { got, threw: String(e?.message) }; }
  }
  return { got };
}

let input = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (c) => { input += c; });
process.stdin.on('end', () => {
  const { fn, args, cleaned } = JSON.parse(input);
  process.stdout.write(JSON.stringify(run(fn, args, cleaned)));
});
