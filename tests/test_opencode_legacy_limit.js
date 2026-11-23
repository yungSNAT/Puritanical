/* global require -- Node.js runtime */
'use strict';

const assert = require('node:assert/strict');
const Module = require('node:module');
const { test } = require('node:test');
const { pathToFileURL } = require('node:url');
const { name } = require('../package.json');
const { repoPath } = require('./helpers/sandbox');

test('OpenCode applies the default size limit to legacy compressed output', async (t) => {
  const plugin = repoPath('.opencode', 'plugins', `${name}.mjs`);
  const originalLoad = Module._load;
  t.after(() => { Module._load = originalLoad; });
  Module._load = function loadLegacyCore(request, parent, isMain) {
    if (parent?.filename === plugin) {
      if (request === `./${name}-hooks/${name}-compress-output.js`) {
        return { compressForOpencode: () => 'compressed' };
      }
      if (request === `./${name}-hooks/${name}-config.js`) return {};
    }
    return originalLoad.call(this, request, parent, isMain);
  };

  const { default: registerPlugin } = await import(pathToFileURL(plugin).href);
  const hooks = await registerPlugin({});
  const after = hooks['tool.execute.after'];
  const atLimit = `[${name}: legacy]`.padEnd(8000, '.');
  const kept = { output: atLimit };
  const rebound = { output: `${atLimit}.` };
  await after({ tool: 'bash' }, kept);
  await after({ tool: 'bash' }, rebound);
  assert.equal(kept.output, atLimit);
  assert.equal(rebound.output, 'compressed');
});
