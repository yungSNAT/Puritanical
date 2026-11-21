#!/usr/bin/env node
/* global Buffer, module, process, require -- Node.js runtime */
// chisle — shared configuration resolver
//
// Resolution order for default mode:
//   1. CHISLE_DEFAULT_MODE environment variable
//   2. User config: $XDG_CONFIG_HOME/chisle/config.json, ~/.config/chisle/config.json
//   3. 'on'

const fs = require('fs');
const path = require('path');
const os = require('os');

// One mode. The intensity levels are gone: three dials on a tool whose whole
// argument is "fewer knobs" was the joke writing itself, and the middle setting
// was the only one ever benchmarked. 'on' or 'off'.
const VALID_MODES = ['off', 'on'];

// Values that meant something before 3.0.0. Kept only so the upgrade can say
// "your setting no longer does anything" instead of ignoring it in silence.
const LEGACY_MODES = ['lite', 'full', 'ultra'];

// Returns the stale value and where it came from, or null. Never throws.
function legacySetting() {
  const env = process.env.CHISLE_DEFAULT_MODE;
  if (env && LEGACY_MODES.includes(String(env).toLowerCase())) {
    return { value: String(env).toLowerCase(), source: 'CHISLE_DEFAULT_MODE' };
  }
  try {
    const p = path.join(getConfigDir(), 'config.json');
    const c = readUserConfig();
    const v = c && c.defaultMode ? String(c.defaultMode).toLowerCase() : null;
    if (v && LEGACY_MODES.includes(v)) return { value: v, source: p };
  } catch {}
  return null;
}

function getClaudeDir() {
  return process.env.CLAUDE_CONFIG_DIR || path.join(os.homedir(), '.claude');
}

// OpenCode's global config dir. The compressor plugin keeps its spill/dedup/
// stats here (as Copilot uses getCopilotDir), so recovery files sit beside
// OpenCode's own config instead of leaking into ~/.claude.
function getOpencodeDir(opts) {
  const options = opts || {};
  const env = options.env || process.env;
  const home = options.home || os.homedir();
  if (env.OPENCODE_CONFIG_DIR) return env.OPENCODE_CONFIG_DIR;
  if (env.XDG_CONFIG_HOME) return path.join(env.XDG_CONFIG_HOME, 'opencode');
  return path.join(home, '.config', 'opencode');
}

function getOpencodeStateDir(opts) {
  const env = (opts && opts.env) || process.env;
  return env.CHISLE_STATE_DIR || getOpencodeDir(opts);
}

// GitHub Copilot CLI's own user-level state directory (mirrors the
// CLAUDE_CONFIG_DIR precedent above). Copilot CLI already reads/writes hook
// config under this same directory (~/.copilot/hooks/), so chisle's
// dedup/spill/stats state lives alongside it rather than under ~/.claude,
// which would be misleading for a machine that never runs Claude Code.
function getCopilotDir() {
  return process.env.COPILOT_HOME || path.join(os.homedir(), '.copilot');
}

function getConfigDir() {
  if (process.env.XDG_CONFIG_HOME) {
    return path.join(process.env.XDG_CONFIG_HOME, 'chisle');
  }
  if (process.platform === 'win32') {
    return path.join(
      process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming'),
      'chisle'
    );
  }
  return path.join(os.homedir(), '.config', 'chisle');
}

// Larger settings are treated as unreadable, just like invalid JSON.
const MAX_CONFIG_BYTES = 4 * 1024 * 1024;

function readSettingsText(filename) {
  // User configuration supports symlinks into a dotfiles checkout.
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Resolve the configured settings file; the reader verifies its type, identity and size.
  const target = fs.realpathSync(filename); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  return readRegularFile(target, MAX_CONFIG_BYTES);
}

function readUserConfig() {
  return JSON.parse(readSettingsText(path.join(getConfigDir(), 'config.json')));
}

function readModeFromConfigFile() {
  try {
    const config = readUserConfig();
    if (config && config.defaultMode &&
        VALID_MODES.includes(String(config.defaultMode).toLowerCase())) {
      return String(config.defaultMode).toLowerCase();
    }
  } catch {}
  return null;
}

function getDefaultMode() {
  const envMode = process.env.CHISLE_DEFAULT_MODE;
  if (envMode && VALID_MODES.includes(envMode.toLowerCase())) {
    return envMode.toLowerCase();
  }
  const userMode = readModeFromConfigFile();
  if (userMode) return userMode;
  return 'on';
}

// ── Section suppression ─────────────────────────────────────────────────────
// Optional config key letting a user drop Chisle's prose rules or code rules
// independently, while keeping the other. Config file only — no env override,
// unlike defaultMode. Absent, malformed, or non-boolean always falls back to
// enabled (true); never throws.
//   { "sections": { "prose": false } }   // suppress prose rules, keep code

// Headings from skills/chisle/SKILL.md, grouped by section.
const PROSE_HEADINGS = [
  'Prose: Maximum Signal Per Token',
  'Output Format',
];
const CODE_HEADINGS = [
  'Code: The Efficiency Ladder',
  'Code Rules',
  'Context Diet: Read Less Into the Window',
];

function readSectionsFromConfigFile() {
  try {
    const config = readUserConfig();
    const s = config && typeof config === 'object' ? config.sections : null;
    return {
      prose: !!(s && typeof s === 'object' && typeof s.prose === 'boolean' ? s.prose : true),
      code: !!(s && typeof s === 'object' && typeof s.code === 'boolean' ? s.code : true),
    };
  } catch {
    return { prose: true, code: true };
  }
}

function getSections() {
  return readSectionsFromConfigFile();
}

// Drops whole `## Heading` sections (heading line through the line before the
// next `## ` heading) whose heading is in an excluded group. Byte-identical
// to `content` when nothing is excluded. Never throws — falls back to the
// unfiltered content. Assumes no `## ` line appears inside a fenced code
// block (true for SKILL.md and the fallback ruleset at 07c603b).
function filterSections(content, sections, proseHeadings, codeHeadings) {
  try {
    if (typeof content !== 'string' || !content) return content;
    const prose = sections ? sections.prose !== false : true;
    const code = sections ? sections.code !== false : true;
    if (prose && code) return content;

    const dropProse = proseHeadings || PROSE_HEADINGS;
    const dropCode = codeHeadings || CODE_HEADINGS;

    const chunks = content.split(/(?=^## .+$)/m);
    const kept = chunks.filter((chunk) => {
      const m = /^## (.+)$/m.exec(chunk);
      if (!m) return true; // preamble chunk (no heading)
      const heading = m[1].trim();
      if (!prose && dropProse.includes(heading)) return false;
      if (!code && dropCode.includes(heading)) return false;
      return true;
    });
    return kept.join('');
  } catch {
    return content;
  }
}

// ── Output-style detection ──────────────────────────────────────────────────
// Claude Code's output-style mechanism also dictates prose structure, so an
// active style and Chisle's prose section issue contradicting instructions with
// no precedence between them (#15). `outputStyle` is a documented settings key
// (schemastore: claude-code-settings.json), and `/output-style` persists the
// selection to .claude/settings.local.json at the local project level, so the
// active style IS readable — it just isn't in the SessionStart payload.
//
// Kept dependency-free on purpose: this file is copied standalone into
// the OpenCode plugin's chisle-hooks/ directory, so it must not require anything
// outside itself.

// Claude Code parses settings.json with strict JSON, but users do leave
// comments in it. Tolerate that rather than silently reading no style.
function parseJsonLoose(raw) {
  try { return JSON.parse(raw.replace(/^\uFEFF/, '')); } catch {}
  try {
    const stripped = raw
      .replace(/^\uFEFF/, '')
      .replace(/"(?:[^"\\]|\\.)*"|\/\*[\s\S]*?\*\/|\/\/[^\n]*/g,
        (m) => (m[0] === '"' ? m : ' '));
    return JSON.parse(stripped);
  } catch { return null; }
}

// Read the same regular file we inspected. The descriptor check also covers
// platforms without O_NOFOLLOW. O_NONBLOCK avoids waiting on a FIFO.
function readRegularFile(filename, maxBytes) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Inspect the entry without following it before opening a regular file.
  const before = fs.lstatSync(filename); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  if (!before.isFile() || before.size > maxBytes) return null;
  const flags = fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0) | (fs.constants.O_NONBLOCK || 0);
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Use no-follow where supported and verify the opened descriptor against the inspected file.
  const fd = fs.openSync(filename, flags); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  try {
    const after = fs.fstatSync(fd);
    if (!after.isFile() || after.dev !== before.dev || after.ino !== before.ino || after.size > maxBytes) return null;
    return readLimitedText(fd, after.size);
  } finally {
    fs.closeSync(fd);
  }
}

function readLimitedText(fd, maxBytes) {
  // Detect concurrent growth without reading beyond the limit plus one byte.
  const buffer = Buffer.alloc(maxBytes + 1);
  let offset = 0;
  while (offset < buffer.length) {
    const count = fs.readSync(fd, buffer, offset, buffer.length - offset, null);
    if (!count) break;
    offset += count;
  }
  return offset <= maxBytes ? buffer.toString('utf8', 0, offset) : null;
}

function readJsonFile(filename, maxBytes = MAX_CONFIG_BYTES) {
  try {
    const raw = readRegularFile(filename, maxBytes);
    return raw === null ? null : parseJsonLoose(raw);
  } catch { return null; }
}

// Claude Code's own precedence, most specific first. A style set for this
// project must beat a different one set globally.
function outputStyleSources(opts) {
  const o = opts || {};
  const cwd = o.cwd || process.cwd();
  const claudeDir = o.claudeDir || getClaudeDir();
  return [
    path.join(cwd, '.claude', 'settings.local.json'),
    path.join(cwd, '.claude', 'settings.json'),
    path.join(claudeDir, 'settings.json'),
  ];
}

// The active output style, or null when none is. "default" is the built-in
// no-op style, so it counts as none.
function getActiveOutputStyle(opts) {
  try {
    for (const file of outputStyleSources(opts)) {
      const json = readJsonFile(file);
      if (!json || typeof json !== 'object') continue;
      const style = json.outputStyle;
      if (typeof style !== 'string') continue;
      const trimmed = style.trim();
      if (!trimmed) continue;
      return trimmed.toLowerCase() === 'default' ? null : trimmed;
    }
    return null;
  } catch { return null; }
}

// What the user wrote, distinguishing "absent" from "explicitly true". Only an
// absent value may be overridden by output-style detection.
function explicitSections() {
  try {
    const json = readJsonFile(path.join(getConfigDir(), 'config.json'));
    const s = json && typeof json === 'object' ? json.sections : null;
    if (!s || typeof s !== 'object') return {};
    const out = {};
    if (typeof s.prose === 'boolean') out.prose = s.prose;
    if (typeof s.code === 'boolean') out.code = s.code;
    return out;
  } catch { return {}; }
}

// Final prose/code decision for the Claude Code hooks. Explicit config always
// wins — a user who wrote `sections.prose: true` has said they want the prose
// rules even alongside a style, and this must not overrule that. Only an unset
// prose key yields to an active style. `code` is never touched: output styles
// govern prose structure, not the efficiency ladder.
//
// Pi has no output-style mechanism, so pi-extension keeps calling getSections().
function resolveSections(opts) {
  const explicit = explicitSections();
  const base = getSections();
  if (explicit.prose !== undefined) return { ...base, outputStyle: null };
  const style = getActiveOutputStyle(opts);
  if (!style) return { ...base, outputStyle: null };
  return { prose: false, code: base.code, outputStyle: style };
}

// Symlink-safe, atomic flag write with 0600 perms.
// Defends against local attacker replacing predictable path with symlink to clobber other files.
function flagWriteDirectory(flagPath) {
  const directory = path.dirname(flagPath);
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Hook callers supply the configured state directory and a fixed state filename.
  fs.mkdirSync(directory, { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Inspect the configured directory entry itself before accepting a symlink.
  if (!fs.lstatSync(directory).isSymbolicLink()) return directory; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Resolve the configured link to check its destination before writing.
  const resolved = fs.realpathSync(directory); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- The resolved target must pass the directory and ownership or home checks below.
  const stat = fs.statSync(resolved); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  if (!stat.isDirectory()) return rejectFlagDirectory('symlink target not a directory');
  if (typeof process.getuid === 'function') {
    if (stat.uid !== process.getuid()) return rejectFlagDirectory('symlink target owned by different uid');
  } else {
    const relative = path.relative(os.homedir(), resolved);
    if (relative === '..' || relative.startsWith('..' + path.sep) || path.isAbsolute(relative)) {
      return rejectFlagDirectory('symlink target outside home dir');
    }
  }
  return resolved;
}

function rejectFlagDirectory(reason) {
  if (process.env.CHISLE_DEBUG === '1') process.stderr.write('[chisle] safeWriteFlag: ' + reason + '\n');
  return null;
}

function safeWriteFlag(flagPath, content) {
  try {
    const realFlagDir = flagWriteDirectory(flagPath);
    if (!realFlagDir) return;

    const realFlagPath = path.join(realFlagDir, path.basename(flagPath));
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Check the destination entry itself and refuse a final symlink.
      if (fs.lstatSync(realFlagPath).isSymbolicLink()) return;
    } catch (e) {
      if (e.code !== 'ENOENT') return;
    }

    const tempPath = path.join(realFlagDir, `.chisle-active.${process.pid}.${Date.now()}`);
    const O_NOFOLLOW = typeof fs.constants.O_NOFOLLOW === 'number' ? fs.constants.O_NOFOLLOW : 0;
    const flags = fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | O_NOFOLLOW;
    let fd;
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Create a new private file exclusively in the configured directory used for state.
      fd = fs.openSync(tempPath, flags, 0o600);
      fs.writeSync(fd, String(content));
      try { fs.fchmodSync(fd, 0o600); } catch {}
    } finally {
      if (fd !== undefined) fs.closeSync(fd);
    }
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Atomically replace the destination entry in the same directory as the private file.
    fs.renameSync(tempPath, realFlagPath);
  } catch {}
}

// Symlink-safe, size-capped, whitelist-validated flag read.
// Refuses symlinks, caps at 64 bytes, rejects unknown modes.
const MAX_FLAG_BYTES = 64;

function readFlag(flagPath) {
  try {
    const text = readRegularFile(flagPath, MAX_FLAG_BYTES);
    if (text === null) return null;
    const mode = text.trim().toLowerCase();
    return VALID_MODES.includes(mode) ? mode : null;
  } catch { return null; }
}

module.exports = {
  LEGACY_MODES, legacySetting, getDefaultMode, getClaudeDir, getCopilotDir, getOpencodeDir, getOpencodeStateDir, getConfigDir,
  VALID_MODES, safeWriteFlag, readFlag, readJsonFile, readSettingsText,
  PROSE_HEADINGS, CODE_HEADINGS, getSections, filterSections,
  getActiveOutputStyle, explicitSections, resolveSections,
};
