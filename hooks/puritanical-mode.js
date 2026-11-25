/* global module, require -- Node.js runtime */
'use strict';

// Shared mode-directive parser backed by puritanical-core.wasm.
// Harness adapters decide how to persist mode.
const coreWasm = require('./puritanical-core-wasm.cjs');

function requestedMode(prompt) {
  if (!prompt) return null;
  return coreWasm.requestedMode(String(prompt));
}

module.exports = { requestedMode };
