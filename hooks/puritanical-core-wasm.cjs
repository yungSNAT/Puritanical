/* global Buffer, WebAssembly, __dirname, module, require -- Node.js runtime */
'use strict';

// Lazy synchronous loader and typed UTF-16LE bindings for `puritanical-core.wasm`.
// CommonJS, Node 18+.

const fs = require('fs');
const path = require('path');

const CLAMP = 0x7fffffff;
const KIND_NAMES = [
  'git-status', 'progress', 'git-log', 'git-diff', 'git-branch', 'ls', 'wc', 'grep', 'test',
];

let cachedExports = null;

function wasm() {
  if (!cachedExports) {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Bundled WebAssembly core stored beside this file.
    const bytes = fs.readFileSync(path.join(__dirname, 'puritanical-core.wasm')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    cachedExports = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports;
  }
  return cachedExports;
}

function allocArg(w, val) {
  if (val == null) return [0, -1];
  const str = String(val);
  const ptr = w.alloc(str.length);
  if (str.length) Buffer.from(w.memory.buffer, ptr, str.length * 2).write(str, 0, 'utf16le');
  return [ptr, str.length];
}

function callWasm(fn, ...vals) {
  const w = wasm();
  const allocated = vals.map(v => allocArg(w, v));
  try {
    return fn(w, ...allocated);
  } finally {
    for (const [ptr, len] of allocated) {
      if (len >= 0) w.dealloc(ptr, len);
    }
  }
}

function readResult(w, len) {
  if (len < 0) return null;
  return Buffer.from(w.memory.buffer, w.result_ptr(), len * 2).toString('utf16le');
}

const encodeHeadings = list => (list ? list.join('\0') : null);
const clampInt = n => Math.min(Math.max(0, Math.trunc(Number(n) || 0)), CLAMP);

function requestedMode(prompt) {
  if (!prompt) return null;
  const code = callWasm((w, [p, l]) => w.requested_mode_u16(p, l), String(prompt));
  return code === 1 ? 'on' : code === 0 ? 'off' : null;
}

function filterSections(content, prose, code, proseHeadings, codeHeadings) {
  return callWasm(
    (w, [cp, cl], [pp, pl], [qp, ql]) => {
      const n = w.filter_sections_u16(cp, cl, prose ? 1 : 0, code ? 1 : 0, pp, pl, qp, ql);
      return n < 0 ? content : readResult(w, n);
    },
    content, encodeHeadings(proseHeadings), encodeHeadings(codeHeadings),
  );
}

const isOldStatusLineCommand = cmd => callWasm((w, [p, l]) => w.is_old_statusline_u16(p, l) !== 0, cmd);
const isOwnedStatusLineCommand = cmd => callWasm((w, [p, l]) => w.is_owned_statusline_u16(p, l) !== 0, cmd);
const unsafeToolName = name => callWasm((w, [p, l]) => w.unsafe_tool_name_u16(p, l) !== 0, String(name));
const containsCredential = text => callWasm((w, [p, l]) => w.contains_credential_u16(p, l) !== 0, String(text));
const scrub = text => callWasm((w, [p, l]) => readResult(w, w.scrub_u16(p, l)), String(text));

function duplicateMarker(toolName, text, reference) {
  return callWasm(
    (w, [tp, tl], [xp, xl]) => readResult(w, w.duplicate_marker_u16(tp, tl, xp, xl, reference === 'previous' ? 0 : 1)),
    String(toolName), String(text),
  );
}

function compress(text, limits, spillPath) {
  const spill = spillPath ? String(spillPath) : null;
  return callWasm(
    (w, [tp, tl], [sp, sl]) => readResult(w, w.compress_u16(
      tp, tl, sp, sl, Math.min(limits.maxChars, CLAMP), Math.min(limits.headLines, CLAMP), Math.min(limits.tailLines, CLAMP),
    )),
    String(text), spill,
  );
}

function boundForOpencode(text, spillPath, maxChars) {
  const spill = spillPath ? String(spillPath) : null;
  const limit = maxChars ? clampInt(maxChars) : 0;
  return callWasm(
    (w, [tp, tl], [sp, sl]) => {
      const n = w.bound_for_opencode_u16(tp, tl, sp, sl, limit);
      return n < 0 ? text : readResult(w, n);
    },
    String(text), spill,
  );
}

const envAssignEnd = (s, start) => callWasm((w, [p, l]) => w.env_assign_end_u16(p, l, clampInt(start)), String(s));

function kindsFor(command) {
  const mask = callWasm((w, [p, l]) => w.kinds_for_u16(p, l), String(command));
  if (!mask) return null;
  const out = new Set();
  KIND_NAMES.forEach((name, bit) => { if (mask & (1 << bit)) out.add(name); });
  return out;
}

const wrappable = segment => callWasm((w, [p, l]) => w.wrappable_u16(p, l) !== 0, String(segment));

function filterCommand(command, text, me) {
  return callWasm(
    (w, [cp, cl], [tp, tl], [mp, ml]) => readResult(w, w.filter_command_u16(cp, cl, tp, tl, mp, ml)),
    String(command), String(text), me,
  );
}

module.exports = {
  requestedMode, filterSections, isOldStatusLineCommand, isOwnedStatusLineCommand,
  unsafeToolName, containsCredential, scrub, duplicateMarker, compress, boundForOpencode,
  envAssignEnd, kindsFor, wrappable, filterCommand,
};
