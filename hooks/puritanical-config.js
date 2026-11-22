#!/usr/bin/env node
/* global Buffer, module, process, require -- Node.js runtime */
// puritanical — shared configuration resolver
//
// Resolution order for default mode:
//   1. PURITANICAL_DEFAULT_MODE environment variable
//   2. User config: $XDG_CONFIG_HOME/puritanical/config.json, ~/.config/puritanical/config.json
//   3. 'on'

const fs = require('fs');
const path = require('path');
const os = require('os');
const crypto = require('crypto');

// Puritanical was formerly Chisle, until 4.0.0. The old name finds what an older
// install left behind, so an upgrade carries it across instead of orphaning it.
const NAME = 'puritanical';
const OLD_NAME = 'chisle'; // formerly Chisle

// A PURITANICAL_* setting wins; the matching CHISLE_* one (formerly Chisle) still applies
// when the new name is unset. Every hook loads this file first.
function aliasOldEnv(env) {
  const oldPrefix = OLD_NAME.toUpperCase() + '_';
  for (const [key, value] of Object.entries(env)) {
    if (!key.startsWith(oldPrefix)) continue;
    const name = NAME.toUpperCase() + '_' + key.slice(oldPrefix.length);
    if (!Object.hasOwn(env, name)) Object.assign(env, { [name]: value });
  }
  return env;
}
aliasOldEnv(process.env);

// One mode. The intensity levels are gone: three dials on a tool whose whole
// argument is "fewer knobs" was the joke writing itself, and the middle setting
// was the only one ever benchmarked. 'on' or 'off'.
const VALID_MODES = ['off', 'on'];

// Values that meant something before 3.0.0. Kept only so the upgrade can say
// "your setting no longer does anything" instead of ignoring it in silence.
const LEGACY_MODES = ['lite', 'full', 'ultra'];

// Returns the stale value and where it came from, or null. Never throws.
function legacySetting() {
  const env = process.env.PURITANICAL_DEFAULT_MODE;
  if (env && LEGACY_MODES.includes(String(env).toLowerCase())) {
    return { value: String(env).toLowerCase(), source: 'PURITANICAL_DEFAULT_MODE' };
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
  if (env.XDG_CONFIG_HOME) return path.join(env.XDG_CONFIG_HOME, 'opencode'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return path.join(home, '.config', 'opencode'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

function getOpencodeStateDir(opts) {
  const env = (opts && opts.env) || process.env;
  return env.PURITANICAL_STATE_DIR || getOpencodeDir(opts);
}

// GitHub Copilot CLI's own user-level state directory (mirrors the
// CLAUDE_CONFIG_DIR precedent above). Copilot CLI already reads/writes hook
// config under this same directory (~/.copilot/hooks/), so puritanical's
// dedup/spill/stats state lives alongside it rather than under ~/.claude,
// which would be misleading for a machine that never runs Claude Code.
function getCopilotDir() {
  return process.env.COPILOT_HOME || path.join(os.homedir(), '.copilot');
}

function configDirFor(name) {
  if (process.env.XDG_CONFIG_HOME) {
    return path.join(process.env.XDG_CONFIG_HOME, name);
  }
  if (process.platform === 'win32') {
    return path.join(
      process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming'),
      name
    );
  }
  return path.join(os.homedir(), '.config', name);
}

function getConfigDir() { return configDirFor(NAME); }
function getOldConfigDir() { return configDirFor(OLD_NAME); }

// ── Upgrade from 3.x (formerly Chisle) ───────────────────────────────────────────
// State entries named after the product, in the Claude, OpenCode and Copilot
// state folders and in the config folder.
const STATE_ENTRIES = ['.@-active', '.@-session-turns', '.@-statusline-suffix', '.@-compress-stats.json',
  '.@-compress-last.json', '.@-trimmed-tools.json', '@-spill', '@-dedup'];

// Renames one entry, link or not, without following it, unless the new name
// is already taken. True when it moved.
function moveIfAbsent(from, to) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- A fixed state name in a state or config folder, inspected without following it.
    fs.lstatSync(from); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The new name beside it must not exist yet.
    if (fs.existsSync(to) || isLink(to)) return false; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The state or config folder the entry moves into.
    fs.mkdirSync(path.dirname(to), { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Renames the entry itself; a link stays a link.
    fs.renameSync(from, to); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    return true;
  } catch { return false; }
}

function isLink(p) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- A fixed state name, inspected without following it.
  try { return fs.lstatSync(p).isSymbolicLink(); } catch { return false; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

// What is old has passed away (2 Corinthians 5:17, NET): each old state entry
// in fromDir takes its new name in dir. Returns the number moved.
function migrateOldState(dir, fromDir = dir) {
  let moved = 0;
  for (const entry of STATE_ENTRIES) {
    if (moveIfAbsent(path.join(fromDir, entry.replace('@', OLD_NAME)), path.join(dir, entry.replace('@', NAME)))) moved++; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  }
  return moved;
}

// Moves config.json and the MCP proxy's state out of the old config folder.
// Its mcp-proxy/ stays for the installer, which rewires the servers using it;
// the old folder goes once it is empty.
function migrateOldConfig() {
  const oldDir = getOldConfigDir();
  const dir = getConfigDir();
  let moved = migrateOldState(dir, oldDir);
  if (moveIfAbsent(path.join(oldDir, 'config.json'), path.join(dir, 'config.json'))) moved++;
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Removes the old config folder only when it is empty.
  try { fs.rmdirSync(oldDir); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  return moved;
}

// The status line an install before 4.0.0 configured, exactly: the old badge
// script alone, run by bash or PowerShell. A status line that does more is the
// user's own and stays.
const OLD_STATUSLINE = /^(?:bash "[^"]*[\\/]chisle-statusline\.sh"|(?:pwsh|powershell) -NoProfile -ExecutionPolicy Bypass -File "[^"]*[\\/]chisle-statusline\.ps1")$/; // formerly Chisle
function isOldStatusLine(sl) {
  return !!sl && typeof sl === 'object' && sl.type === 'command' && typeof sl.command === 'string' &&
    Object.keys(sl).length === 2 && OLD_STATUSLINE.test(sl.command);
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
  const envMode = process.env.PURITANICAL_DEFAULT_MODE;
  if (envMode && VALID_MODES.includes(envMode.toLowerCase())) {
    return envMode.toLowerCase();
  }
  const userMode = readModeFromConfigFile();
  if (userMode) return userMode;
  return 'on';
}

// ── Section suppression ─────────────────────────────────────────────────────
// Optional config key letting a user drop Puritanical's prose rules or code rules
// independently, while keeping the other. Config file only — no env override,
// unlike defaultMode. Absent, malformed, or non-boolean always falls back to
// enabled (true); never throws.
//   { "sections": { "prose": false } }   // suppress prose rules, keep code

// Headings from skills/puritanical/SKILL.md, grouped by section.
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
// active style and Puritanical's prose section issue contradicting instructions with
// no precedence between them (#15). `outputStyle` is a documented settings key
// (schemastore: claude-code-settings.json), and `/output-style` persists the
// selection to .claude/settings.local.json at the local project level, so the
// active style IS readable — it just isn't in the SessionStart payload.
//
// Kept dependency-free on purpose: this file is copied standalone into
// the OpenCode plugin's puritanical-hooks/ directory, so it must not require anything
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
    path.join(cwd, '.claude', 'settings.local.json'), // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    path.join(cwd, '.claude', 'settings.json'), // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    path.join(claudeDir, 'settings.json'), // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
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
  if (process.env.PURITANICAL_DEBUG === '1') process.stderr.write('[puritanical] safeWriteFlag: ' + reason + '\n');
  return null;
}

function safeWriteFlag(flagPath, content) {
  try {
    const realFlagDir = flagWriteDirectory(flagPath);
    if (!realFlagDir) return;

    const realFlagPath = path.join(realFlagDir, path.basename(flagPath)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Check the destination entry itself and refuse a final symlink.
      if (fs.lstatSync(realFlagPath).isSymbolicLink()) return; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    } catch (e) {
      if (e.code !== 'ENOENT') return;
    }

    const tempPath = path.join(realFlagDir, `.puritanical-active.${process.pid}.${Date.now()}`); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    const O_NOFOLLOW = typeof fs.constants.O_NOFOLLOW === 'number' ? fs.constants.O_NOFOLLOW : 0;
    const flags = fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | O_NOFOLLOW;
    let fd;
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Create a new private file exclusively in the configured directory used for state.
      fd = fs.openSync(tempPath, flags, 0o600); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      fs.writeSync(fd, String(content));
      try { fs.fchmodSync(fd, 0o600); } catch {}
    } finally {
      if (fd !== undefined) fs.closeSync(fd);
    }
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Atomically replace the destination entry in the same directory as the private file.
    fs.renameSync(tempPath, realFlagPath); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
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

// State files under a user-owned root (compression dedup, spill, stats).
// `root` may itself be a symlink (e.g. dotfiles managed with GNU stow), so it
// is resolved with stat — but every subdirectory beneath it and the target
// file are verified with lstat and must not be links.
function secureDirectory(dir, root) {
  try {
    dir = path.resolve(dir); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    root = root ? path.resolve(root) : dir; // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- State directory chosen by the hook (Claude, OpenCode or Copilot); every level up to its root is checked below.
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    let cur = dir;
    while (cur !== root && cur.length > 1) {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Walks from the state directory up to its root and refuses any link on the way.
      const st = fs.lstatSync(cur); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      if (!st.isDirectory() || st.isSymbolicLink()) return false;
      const parent = path.dirname(cur);
      if (parent === cur) break;
      cur = parent;
    }
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The configured state root itself, which may be a deliberate symlink such as a stow-managed dotfile.
    const st = fs.statSync(root); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    return st.isDirectory();
  } catch { return false; }
}

// A path that exists must be a regular file, not a link; a missing one is fine.
function replaceableFile(p) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- State file inside a checked directory; a link or non-file is refused.
    const st = fs.lstatSync(p); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    return !st.isSymbolicLink() && st.isFile();
  } catch (e) {
    return e.code === 'ENOENT';
  }
}

function safeAtomicWrite(p, content, root) {
  const dir = path.dirname(p);
  if (!secureDirectory(dir, root) || !replaceableFile(p)) return false;
  const tmp = p + '.' + process.pid + '.' + crypto.randomBytes(6).toString('hex') + '.tmp';
  const noFollow = typeof fs.constants.O_NOFOLLOW === 'number' ? fs.constants.O_NOFOLLOW : 0;
  const flags = fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | noFollow;
  let fd;
  let created = false;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Exclusive no-follow create of a random temporary file beside the checked state file.
    fd = fs.openSync(tmp, flags, 0o600); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    created = true;
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Writes through the descriptor opened exclusively above.
    fs.writeFileSync(fd, content); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    try { fs.fchmodSync(fd, 0o600); } catch {}
    fs.closeSync(fd);
    fd = undefined;
    if (!replaceableFile(p)) return false;
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Atomically replaces the state file, rechecked as replaceable just above.
    fs.renameSync(tmp, p); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    created = false;
    return true;
  } catch {
    return false;
  } finally {
    if (fd !== undefined) { try { fs.closeSync(fd); } catch {} }
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Removes this process's own temporary file after a failed write.
    if (created) { try { fs.unlinkSync(tmp); } catch {} } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  }
}

module.exports = {
  NAME, OLD_NAME, aliasOldEnv, getOldConfigDir, migrateOldState, migrateOldConfig, isOldStatusLine,
  LEGACY_MODES, legacySetting, getDefaultMode, getClaudeDir, getCopilotDir, getOpencodeDir, getOpencodeStateDir, getConfigDir,
  VALID_MODES, safeWriteFlag, readFlag, readJsonFile, readRegularFile, readSettingsText, secureDirectory, safeAtomicWrite,
  PROSE_HEADINGS, CODE_HEADINGS, getSections, filterSections,
  getActiveOutputStyle, explicitSections, resolveSections,
};
