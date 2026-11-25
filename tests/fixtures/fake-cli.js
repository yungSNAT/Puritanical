#!/usr/bin/env node
/* global process, require -- Node.js runtime */
// Stand-in for the claude, gemini and pi CLIs in the upgrade tests:
//   node fake-cli.js <name> <root> <args...>
// State lives in <root>/<name>.state.json and every call is logged, one line
// each, to <root>/<name>.log. A claude uninstall fails for scopes in `fail`.
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const [name, root, ...args] = process.argv.slice(2);
const stateFile = path.join(root, name + '.state.json');
const state = { plugins: [], marketplaces: [], extensions: [], packages: [], fail: [], ...JSON.parse(fs.readFileSync(stateFile, 'utf8')) };
fs.appendFileSync(path.join(root, name + '.log'), args.join(' ') + '\n');
const line = args.join(' ');
const out = text => { process.stdout.write(text); return 0; };

// Removes the entries matching gone from state[key]; returns the exit status.
function drop(key, gone) {
  const before = state[key].length;
  state[key] = state[key].filter(entry => !gone(entry));
  fs.writeFileSync(stateFile, JSON.stringify(state));
  return state[key].length < before ? 0 : 1;
}

function add(key, entry) {
  if (!state[key].includes(entry)) state[key].push(entry);
  fs.writeFileSync(stateFile, JSON.stringify(state));
  return 0;
}

function claude() {
  const plugins = state.plugins.map(p => ({ scope: 'user', ...p }));
  if (line === 'plugin list --json') return out(JSON.stringify(plugins));
  if (line === 'plugin list') return out(plugins.map(p => `  ${p.id}\n    Scope: ${p.scope}\n`).join(''));
  if (line === 'plugin marketplace list --json') return out(JSON.stringify(state.marketplaces.map(n => ({ name: n }))));
  const scope = args.includes('--scope') ? args[args.indexOf('--scope') + 1] : 'user';
  if (args[1] === 'uninstall') {
    if (state.fail.includes(scope)) return 1;
    return drop('plugins', p => p.id === args[2] && (p.scope || 'user') === scope);
  }
  if (args[1] === 'install') {
    if (plugins.some(p => p.id === args[2] && p.scope === 'user')) return 0;
    return add('plugins', { id: args[2] });
  }
  if (args[1] === 'marketplace' && args[2] === 'add') return add('marketplaces', args[3].split('/').pop().toLowerCase());
  if (args[1] === 'marketplace' && args[2] === 'remove') return drop('marketplaces', n => n === args[3]);
  return 0;
}

function gemini() {
  if (line === 'extensions list') return out(state.extensions.map(e => e + ' (1.0.0)\n').join(''));
  if (args[1] === 'uninstall') {
    if (state.fail.includes('uninstall')) return 1;
    return drop('extensions', e => e === args[2]);
  }
  if (args[1] === 'install') return add('extensions', args[2].split('/').pop().toLowerCase());
  return 0;
}

function pi() {
  if (line === 'list') return out('User packages:\n' + state.packages.map(p => '  ' + p + '\n').join(''));
  if (args[0] === 'remove') {
    if (state.fail.includes('uninstall')) return 1;
    return drop('packages', p => p.replace(/@[^/@]+$/, '') === args[1]);
  }
  if (args[0] === 'install') return add('packages', args[1]);
  return 0;
}

process.exitCode = { claude, gemini, pi }[name]();
