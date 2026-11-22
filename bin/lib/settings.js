/* global module, process, require -- Node.js runtime */
// puritanical — JSONC-tolerant settings.json read/write + defensive hook validation.
//
// Claude Code applies strict Zod to settings.json: one malformed hook silently
// discards the WHOLE file. So every mutation validates before write, and reads
// tolerate the // and /* */ comments users sometimes add.
//
// Public API:
//   readSettings(path)                  → object, {}, or null on hard parse failure
//   writeSettings(path, obj)            → atomic write, mode 0600
//   stripJsonComments(src)              → comment + trailing-comma stripped
//   validateHookFields(settings)        → mutates: drops malformed hook entries
//   hasHook(settings, ev, marker)       → idempotency probe
//   addCommandHook(settings, ev, opts)  → no-op if marker already present
//   removeHooks(settings, marker)       → uninstall helper
//
// Pure stdlib, CommonJS, Node ≥18.

'use strict';

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { readSettingsText } = require('../../hooks/puritanical-config');

// ── stripJsonComments ──────────────────────────────────────────────────────
// Scanner that copies quoted strings whole (backslash escapes included) so a
// comment-looking sequence inside one survives. Trailing commas swept in a
// final pass.

// Index just past the string opened at `start`, or the end if it never closes.
function stringEnd(src, start) {
  const q = src.charAt(start);
  for (let j = start + 1; j < src.length; j++) {
    if (src.charAt(j) === '\\') j++;
    else if (src.charAt(j) === q) return j + 1;
  }
  return src.length;
}

// Index of `token` at or after `from`, or the end if absent.
function indexOrEnd(src, token, from) {
  const at = src.indexOf(token, from);
  return at < 0 ? src.length : at;
}

function stripJsonComments(src) {
  if (typeof src !== 'string') return src;
  let out = '';
  let i = 0;
  while (i < src.length) {
    const c = src.charAt(i);
    const pair = src.slice(i, i + 2);
    if (c === '"' || c === "'") {
      const end = stringEnd(src, i);
      out += src.slice(i, end);
      i = end;
    } else if (pair === '//') {
      i = indexOrEnd(src, '\n', i + 2);   // the newline itself is kept
    } else if (pair === '/*') {
      i = Math.min(src.length, indexOrEnd(src, '*/', i + 2) + 2);
    } else {
      out += c;
      i++;
    }
  }
  return out.replace(/,(\s*[}\]])/g, '$1');
}

// ── readSettings ───────────────────────────────────────────────────────────
// Strict JSON fast path → JSONC fallback → null (never silently clobber a
// recoverable file with {}).
function readSettings(p) {
  let raw;
  try { raw = readSettingsText(p); }
  catch (error) {
    if (error.code === 'ENOENT') return {};
    process.stderr.write(`puritanical: cannot read ${p}: ${error.message}\n`);
    return null;
  }
  if (raw === null) {
    process.stderr.write(`puritanical: cannot read ${p}: expected a bounded, regular file\n`);
    return null;
  }
  if (!raw.trim()) return {};
  try { return JSON.parse(raw.replace(/^\uFEFF/, '')); } catch {}
  try { return JSON.parse(stripJsonComments(raw.replace(/^\uFEFF/, ''))); }
  catch (e) {
    process.stderr.write(`puritanical: warning — ${p} is not valid JSON/JSONC: ${e.message}\n`);
    return null;
  }
}

// ── writeSettings ──────────────────────────────────────────────────────────
// Atomic: temp + rename. mode 0600 (settings often holds tokens).
function writeAtomically(p, text, { mode = 0o600, noReplace = false } = {}) {
  const dir = path.dirname(p);
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Parent directory of a settings, MCP config or trim record file chosen by the installer.
  fs.mkdirSync(dir, { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  const tmp = path.join(dir, `.${path.basename(p)}.${process.pid}.${crypto.randomBytes(8).toString('hex')}.tmp`); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Exclusive create of a random temporary name beside the installer's target file.
  const fd = fs.openSync(tmp, 'wx', mode); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  let renamed = false;
  try {
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Writes through the descriptor of the temporary file opened exclusively above.
      fs.writeFileSync(fd, text); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    } finally {
      fs.closeSync(fd);
    }
    if (noReplace) publishNew(tmp, p);
    else {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Atomically replaces the installer's target file with the complete temporary file.
      fs.renameSync(tmp, p); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      renamed = true;
    }
  } finally {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Removes this process's own temporary file after a failed publish.
    if (!renamed) { try { fs.unlinkSync(tmp); } catch {} } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  }
}

// Publishes a complete backup without replacing an existing one: a hard link,
// or an exclusive copy on filesystems without hard links (exFAT, some SMB or
// 9p shares). EEXIST from either means the backup already exists.
function publishNew(tmp, p) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Publishes the complete temporary file at the backup path without replacing an existing file.
    fs.linkSync(tmp, p); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  } catch (error) {
    if (error.code === 'EEXIST') throw error;
    fs.copyFileSync(tmp, p, fs.constants.COPYFILE_EXCL); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  }
}

function writeSettings(p, obj) {
  writeAtomically(p, JSON.stringify(obj, null, 2) + '\n');
}

// ── validateHookFields ─────────────────────────────────────────────────────
// Mutate-to-valid before write. Drops any entry Claude Code's Zod would reject.
function validateHookFields(settings) {
  if (!settings || typeof settings !== 'object') return settings;
  if (!settings.hooks || typeof settings.hooks !== 'object') return settings;
  const kept = Object.entries(settings.hooks)
    .filter(([, arr]) => Array.isArray(arr))
    .map(([ev, arr]) => [ev, arr.filter(validEntry)])
    .filter(([, arr]) => arr.length > 0);
  setHooks(settings, kept);
  return settings;
}

function validEntry(entry) {
  if (!entry || typeof entry !== 'object' || !Array.isArray(entry.hooks)) return false;
  entry.hooks = entry.hooks.filter(h => !!h && typeof h === 'object' && h.type === 'command'
    && typeof h.command === 'string' && h.command.length > 0);
  return entry.hooks.length > 0;
}

// Replaces settings.hooks with [event, entries] pairs; no pairs drops the key.
function setHooks(settings, pairs) {
  if (pairs.length) settings.hooks = Object.fromEntries(pairs);
  else delete settings.hooks;
}

// Own entries for one hook event; event names are Claude Code's fixed set
// (SessionStart, PostToolUse, ...) passed by the installer.
function eventHooks(settings, event) {
  const hooks = settings && settings.hooks;
  if (!hooks || !Object.hasOwn(hooks, event)) return undefined;
  // eslint-disable-next-line security/detect-object-injection -- Own property named by a fixed Claude Code hook event.
  return hooks[event];
}

// ── idempotency probe ──────────────────────────────────────────────────────
function hasHook(settings, event, marker) {
  const arr = eventHooks(settings, event);
  if (!Array.isArray(arr)) return false;
  return arr.some(e => e && Array.isArray(e.hooks) &&
    e.hooks.some(h => h && typeof h.command === 'string' && h.command.includes(marker)));
}

// ── addCommandHook ─────────────────────────────────────────────────────────
function addCommandHook(settings, event, opts) {
  if (!settings.hooks) settings.hooks = {};
  if (!Array.isArray(eventHooks(settings, event))) Object.assign(settings.hooks, { [event]: [] });
  const marker = opts.marker || opts.command;
  if (hasHook(settings, event, marker)) return false;
  const hook = { type: 'command', command: opts.command };
  if (typeof opts.timeout === 'number') hook.timeout = opts.timeout;
  if (typeof opts.statusMessage === 'string') hook.statusMessage = opts.statusMessage;
  const entry = { hooks: [hook] };
  if (typeof opts.matcher === 'string') entry.matcher = opts.matcher;
  eventHooks(settings, event).push(entry);
  return true;
}

// ── removeHooks (uninstall) ────────────────────────────────────────────────
function removeHooks(settings, marker) {
  if (!settings || !settings.hooks) return 0;
  validateHookFields(settings);
  if (!settings.hooks) return 0;
  let removed = 0;
  const kept = [];
  for (const [ev, arr] of Object.entries(settings.hooks)) {
    if (!Array.isArray(arr)) continue;
    const left = arr.filter(entry => !entry || !Array.isArray(entry.hooks)
      || !entry.hooks.some(h => h && typeof h.command === 'string' && h.command.includes(marker)));
    removed += arr.length - left.length;
    if (left.length) kept.push([ev, left]);
  }
  setHooks(settings, kept);
  return removed;
}

module.exports = {
  stripJsonComments, readSettings, writeSettings, writeAtomically,
  validateHookFields, hasHook, addCommandHook, removeHooks,
};
