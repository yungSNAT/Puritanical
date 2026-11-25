/* global Buffer, WebAssembly, __dirname -- Node.js runtime */
'use strict';

// Lazy synchronous loader and typed UTF-16LE bindings for `puritanical-core.wasm`.
// CommonJS, Node 18+.

const fs = require('fs');
const path = require('path');

const CLAMP = 0x7fffffff;

let cachedExports = null;

function wasm() {
  if (!cachedExports) {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Bundled WebAssembly core stored beside this file.
    const bytes = fs.readFileSync(path.join(__dirname, 'puritanical-core.wasm')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    cachedExports = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;
  }
  return cachedExports;
}

function writeArg(w, slot, val) {
  if (val == null) return 0;
  const str = String(val);
  const ptr = w.arg_buf(slot, str.length);
  if (str.length) Buffer.from(w.memory.buffer, ptr, str.length * 2).write(str, 0, 'utf16le');
  return 1;
}

function callWasm(fn, ...vals) {
  const w = wasm();
  const flags = vals.map((v, i) => writeArg(w, i, v));
  return fn(w, ...flags);
}

function readResult(w, len) {
  if (len < 0) return null;
  return Buffer.from(w.memory.buffer, w.result_ptr(), len * 2).toString('utf16le');
}

// Trailing NUL preserves `['']` distinctly from `[]` (empty string) and `null` (default).
const encodeHeadings = list => (list ? (list.length ? `${list.join('\0')}\0` : '') : null);
const clampInt = n => Math.min(Math.max(0, Math.trunc(Number(n) || 0)), CLAMP);

function requestedMode(prompt) {
  if (!prompt) return null;
  const code = callWasm(w => w.requested_mode_u16(), String(prompt));
  return code === 1 ? 'on' : code === 0 ? 'off' : null;
}

function filterSections(content, prose, code, proseHeadings, codeHeadings) {
  return callWasm(
    (w, _hasC, hasP, hasQ) => {
      const n = w.filter_sections_u16(prose ? 1 : 0, code ? 1 : 0, hasP, hasQ);
      return n < 0 ? content : readResult(w, n);
    },
    content, encodeHeadings(proseHeadings), encodeHeadings(codeHeadings),
  );
}

const isOldStatusLineCommand = cmd => callWasm(w => w.is_old_statusline_u16() !== 0, cmd);
const isOwnedStatusLineCommand = cmd => callWasm(w => w.is_owned_statusline_u16() !== 0, cmd);
const unsafeToolName = name => callWasm(w => w.unsafe_tool_name_u16() !== 0, String(name));
const containsCredential = text => callWasm(w => w.contains_credential_u16() !== 0, String(text));
const scrub = text => callWasm(w => readResult(w, w.scrub_u16()), String(text));

function duplicateMarker(toolName, text, reference) {
  return callWasm(
    w => readResult(w, w.duplicate_marker_u16(reference === 'previous' ? 0 : 1)),
    String(toolName), String(text),
  );
}

function compress(text, limits, spillPath) {
  const spill = spillPath ? String(spillPath) : null;
  return callWasm(
    (w, _hasT, hasSpill) => readResult(w, w.compress_u16(
      hasSpill, clampInt(limits.maxChars), clampInt(limits.headLines), clampInt(limits.tailLines),
    )),
    String(text), spill,
  );
}

function boundForOpencode(text, spillPath, maxChars) {
  const spill = spillPath ? String(spillPath) : null;
  const limit = maxChars ? clampInt(maxChars) : 0;
  return callWasm(
    (w, _hasT, hasSpill) => {
      const n = w.bound_for_opencode_u16(hasSpill, limit);
      return n < 0 ? text : readResult(w, n);
    },
    String(text), spill,
  );
}

const envAssignEnd = (s, start) => callWasm(w => w.env_assign_end_u16(clampInt(start)), String(s));

function kindsFor(command) {
  const csv = callWasm(w => readResult(w, w.kinds_for_u16()), String(command));
  return csv ? new Set(csv.split(',')) : null;
}

const wrappable = segment => callWasm(w => w.wrappable_u16() !== 0, String(segment));

function filterCommand(command, text, me) {
  return callWasm(
    (w, _hasC, _hasT, hasMe) => readResult(w, w.filter_command_u16(hasMe)),
    String(command), String(text), me,
  );
}

module.exports = {
  requestedMode, filterSections, isOldStatusLineCommand, isOwnedStatusLineCommand,
  unsafeToolName, containsCredential, scrub, duplicateMarker, compress, boundForOpencode,
  envAssignEnd, kindsFor, wrappable, filterCommand,
};
