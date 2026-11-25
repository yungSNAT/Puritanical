/* global module, require -- Node.js runtime */
// puritanical — command-aware output filters (RTK-style, post-hoc), backed by
// puritanical-core.wasm. See core/src/filters.rs for matching and rules.

'use strict';

const coreWasm = require('./puritanical-core-wasm.cjs');

// CSI and OSC escapes; kept exported for compatibility.
// eslint-disable-next-line no-control-regex -- ANSI escapes are delimited by the ESC and BEL control characters.
const ANSI_RE = /\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g;

let me = null;
try { me = require('os').userInfo().username; } catch {}

const envAssignEnd = (s, start) => coreWasm.envAssignEnd(s, start);
const wrappable = segment => coreWasm.wrappable(segment);

function kindsFor(command) {
  if (typeof command !== 'string' || !command.trim()) return null;
  return coreWasm.kindsFor(command);
}

function filterCommand(command, text) {
  if (typeof text !== 'string' || !text || typeof command !== 'string' || !command.trim()) return null;
  return coreWasm.filterCommand(command, text, me);
}

module.exports = { filterCommand, kindsFor, wrappable, envAssignEnd, ANSI_RE };
