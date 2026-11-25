/* global module, process, require, __dirname -- Node.js runtime */
// puritanical installer: platform, process and directory helpers shared by the
// install, uninstall and update paths.

'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const cp = require('child_process');
const { getOpencodeDir, getOpencodeStateDir } = require('../../hooks/puritanical-config');

// This fork. The Claude marketplace, Gemini extension and pi package all come
// from it, not from the upstream Chisle repository or npm package.
const REPO = 'yungSNAT/Puritanical';
const IS_WIN = process.platform === 'win32';
// Repo root = parent of bin/. Installed package or local clone both work.
const REPO_ROOT = path.resolve(__dirname, '..', '..');

function die(msg) { process.stderr.write(msg + '\n'); process.exit(2); }

function expandHome(p) { return p.replace(/^~/, os.homedir()).replace(/^\$HOME/, os.homedir()); }

// POSIX `command -v` without a shell: an executable regular file in a PATH
// directory. Empty PATH entries (the current directory) are not searched.
function onPath(cmd) {
  for (const dir of (process.env.PATH || '').split(path.delimiter)) {
    if (!dir) continue;
    const file = path.join(dir, cmd); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    try {
      fs.accessSync(file, fs.constants.X_OK);
      if (isFile(file)) return true;
    } catch {}
  }
  return false;
}

function hasCmd(cmd) {
  try {
    // Detection asks where.exe to search the user's PATH by design (Sonar S4036).
    if (IS_WIN) return cp.spawnSync('where', [cmd], { stdio: 'ignore' }).status === 0; // NOSONAR
    return onPath(cmd);
  } catch { return false; }
}

function statOf(p) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Detection probes fixed agent directories under the home directory or PATH.
  try { return fs.statSync(p); } catch { return null; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}
function isDir(p) { const st = statOf(p); return !!st && st.isDirectory(); }
function isFile(p) { const st = statOf(p); return !!st && st.isFile(); }

// Windows command-line quoting: backslashes before a quote or the closing
// quote are doubled, and each quote gets one more backslash.
function quoteWin(a) {
  if (!IS_WIN) return a;
  if (a !== '' && !/[\s"]/.test(a)) return a;
  let out = '"';
  let slashes = 0;
  for (const ch of String(a)) {
    if (ch === '\\') { slashes++; continue; }
    out += ch === '"' ? '\\'.repeat(slashes * 2 + 1) + ch : '\\'.repeat(slashes) + ch;
    slashes = 0;
  }
  return out + '\\'.repeat(slashes * 2) + '"';
}
function spawnX(cmd, args, o) {
  // .cmd shims (npm-installed agent CLIs) need cmd.exe on Windows. The command
  // is one of the installer's fixed agent CLIs; arguments are installer
  // constants or package sources limited to package-spec characters
  // (uninstall.js piSources), so cmd.exe sees no metacharacters.
  if (IS_WIN) return cp.spawnSync(`${cmd} ${args.map(quoteWin).join(' ')}`, [], Object.assign({ shell: true }, o || {})); // nosemgrep: javascript_exec_rule-child-process, javascript.lang.security.detect-child-process.detect-child-process
  return cp.spawnSync(cmd, args, o || {}); // nosemgrep: javascript_exec_rule-child-process, javascript.lang.security.detect-child-process.detect-child-process
}
function run(cmd, args, dry) {
  if (dry) { process.stdout.write(`  would run: ${cmd} ${args.join(' ')}\n`); return { status: 0 }; }
  process.stdout.write(`  $ ${cmd} ${args.join(' ')}\n`);
  return spawnX(cmd, args, { stdio: 'inherit' });
}

// spawnSync reports a missing binary as { status: null, error: ENOENT } and a
// signal death as { status: null, signal }. `(r.status || 0) === 0` reads both
// as success, so a machine without the CLI was told the install succeeded.
// Only an explicit exit code of 0 counts.
function ranOk(r) {
  return !!r && !r.error && r.status === 0;
}
const CAPTURE_TIMEOUT_MS = 15000;
function capture(cmd, args, timeout = CAPTURE_TIMEOUT_MS) {
  try { return spawnX(cmd, args, { encoding: 'utf8', timeout, killSignal: 'SIGKILL' }); } catch { return { status: 1, stdout: '', stderr: '' }; }
}
// True when `<cmd> <args>` exits 0 and its output matches pattern, by default
// "puritanical" (plugin/extension lists).
function listsPuritanical(cmd, args, pattern = /puritanical/i) {
  const r = capture(cmd, args);
  return r.status === 0 && pattern.test(r.stdout || '');
}

// The Claude plugin and Gemini extension as installs before 4.0.0 named them.
const OLD_PLUGIN = 'chisle@chisle'; // formerly Chisle
const OLD_LISTED = /\bchisle\b/i; // formerly Chisle

function claudeDir(opts) {
  if (opts.configDir) return opts.configDir;
  if (process.env.CLAUDE_CONFIG_DIR) return process.env.CLAUDE_CONFIG_DIR;
  return path.join(os.homedir(), '.claude');
}

// Home dir for Puritanical-managed global targets. PURITANICAL_HOME overrides it
// (tests, containers); mirrors the CLAUDE_CONFIG_DIR precedent for Claude.
function homeDir() { return process.env.PURITANICAL_HOME || os.homedir(); }

function opencodeDir() { return getOpencodeDir({ home: homeDir() }); }
// Copilot CLI reads hooks, instructions and MCP config under COPILOT_HOME.
function copilotDir() { return process.env.COPILOT_HOME || path.join(homeDir(), '.copilot'); }
function opencodeStateDir() { return getOpencodeStateDir({ home: homeDir() }); }
function opencodeMajor() {
  let r = capture('opencode', ['--version']);
  if (!ranOk(r)) r = capture('opencode2', ['--version']);
  const match = ranOk(r) && String(r.stdout || '').match(/(\d+)\./);
  return match ? Number(match[1]) : 1;
}

module.exports = {
  REPO, IS_WIN, REPO_ROOT, OLD_PLUGIN, OLD_LISTED, CAPTURE_TIMEOUT_MS, die, expandHome, hasCmd, isDir, isFile, quoteWin,
  run, ranOk, capture, listsPuritanical, claudeDir, homeDir, opencodeDir, copilotDir,
  opencodeStateDir, opencodeMajor,
};
