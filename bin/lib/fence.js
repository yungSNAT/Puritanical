/* global module, process, require -- Node.js runtime */
// puritanical installer: fenced AGENTS.md rulesets and Agent-Skills copies.
//
// Targets are global agent files under the user's home or agent config
// directories (PURITANICAL_HOME, CODEX/OpenCode/Copilot roots); sources are files in
// this package.

'use strict';

const fs = require('fs');
const path = require('path');
const { REPO_ROOT } = require('./env');

const FENCE_BEGIN = '<!-- puritanical-begin -->';
const FENCE_END = '<!-- puritanical-end -->';

// Begin and end offsets of the first puritanical block; e is -1 when the block is
// missing or has no end marker.
function fenceSpan(txt) {
  const b = txt.indexOf(FENCE_BEGIN);
  return { b, e: b === -1 ? -1 : txt.indexOf(FENCE_END, b) };
}

function readText(file) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Agent instruction files under the user's home or agent config root.
  return fs.readFileSync(file, 'utf8'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function writeText(file, text) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Agent instruction files under the user's home or agent config root.
  fs.writeFileSync(file, text, { mode: 0o644 }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function exists(p) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Paths under the user's agent config roots or this package.
  return fs.existsSync(p); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function mkdirp(dir) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Agent config subdirectories under the user's home or agent config root.
  fs.mkdirSync(dir, { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function removeTree(p) {
  fs.rmSync(p, { recursive: true, force: true });
}

function copyFile(src, dst) {
  fs.copyFileSync(src, dst); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

// Fenced AGENTS.md append/refresh for Codex-style targets. Appends after
// existing content (never prepends, never clobbers), refreshes one fenced
// block in place under --force. Returns 'installed', 'refreshed', 'skipped'.
function writeFencedRuleset(target, opts, note) {
  const raw = readText(path.join(REPO_ROOT, 'AGENTS.md'));
  const block = FENCE_BEGIN + '\n' + raw.trimEnd() + '\n' + FENCE_END + '\n';
  mkdirp(path.dirname(target));
  let existing = '';
  try { existing = readText(target); } catch {}
  if (existing.indexOf(FENCE_BEGIN) === -1) {
    writeText(target, existing + separator(existing) + block);
    process.stdout.write('  installed: ' + target + '\n');
    return 'installed';
  }
  if (!opts.force) {
    note('  ' + target + ' already contains puritanical ruleset (--force to refresh)');
    return 'skipped';
  }
  const { b, e } = fenceSpan(existing);
  // A begin marker with no end marker means the file was hand-edited or a
  // write was cut short. Everything after the begin marker is then user
  // content of unknown extent, so refuse rather than guess: rewriting would
  // delete the rest of their global instructions.
  if (e === -1) {
    note('  ' + target + ' has a puritanical-begin marker with no puritanical-end; leaving it alone');
    note('  remove the stray marker by hand, then re-run');
    return 'skipped';
  }
  let tail = existing.slice(e + FENCE_END.length);
  if (tail.charAt(0) === '\n') tail = tail.slice(1);
  writeText(target, existing.slice(0, b) + block + tail);
  process.stdout.write('  refreshed ruleset in ' + target + '\n');
  return 'refreshed';
}

function separator(existing) {
  if (!existing || existing.slice(-2) === '\n\n') return '';
  return existing.slice(-1) === '\n' ? '\n' : '\n\n';
}

// Text with the puritanical block cut out, or null when there is no complete block.
function withoutFence(txt) {
  const { b, e } = fenceSpan(txt);
  if (e === -1) return null;
  const tail = txt.slice(e + FENCE_END.length).replace(/^\n/, '');
  return (txt.slice(0, b) + tail).replace(/\n{3,}/g, '\n\n');
}

// Skills-root helpers for Agent-Skills hosts (OpenCode, Hermes). Both load
// one dir per skill through the host's native `skill` tool, so the bundled
// skills/ tree copies over verbatim: no conversion, no second system
// message, existing user instructions untouched.
function ownedSkillNames() {
  const srcRoot = path.join(REPO_ROOT, 'skills');
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- This package's own skills/ directory.
    return fs.readdirSync(srcRoot).filter(n => exists(path.join(srcRoot, n, 'SKILL.md')) && isDirEntry(path.join(srcRoot, n))); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  } catch { return []; }
}

function isDirEntry(p) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Entries of this package's skills/ directory or a package skill directory.
  try { return fs.statSync(p).isDirectory(); } catch { return false; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function statFile(p) {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Files inside one of this package's skill directories.
  try { return fs.statSync(p).isFile(); } catch { return false; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

// Overwrites stale files, keeps foreign siblings, never touches files
// outside the named skill dirs. Returns files written.
function copySkills(dstRoot, opts) {
  let written = 0;
  for (const name of ownedSkillNames()) {
    const src = path.join(REPO_ROOT, 'skills', name);
    const dst = path.join(dstRoot, name); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- A skill directory shipped in this package.
    const files = fs.readdirSync(src); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    if (!opts.dryRun) mkdirp(dst);
    for (const f of files) {
      if (!statFile(path.join(src, f))) continue;
      if (!opts.dryRun) copyFile(path.join(src, f), path.join(dst, f)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      written++;
    }
  }
  return written;
}

// Puritanical-owned skill dirs present under dstRoot.
function presentSkills(dstRoot) {
  return ownedSkillNames().filter(name => exists(path.join(dstRoot, name))); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

// Removes only the skill dirs Puritanical owns; foreign skills stay put. Returns
// the count removed (or that would be removed on a dry run).
function pruneSkills(dstRoot, dryRun) {
  const present = presentSkills(dstRoot);
  if (!dryRun) for (const name of present) removeTree(path.join(dstRoot, name)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return present.length;
}

module.exports = {
  FENCE_BEGIN, FENCE_END, fenceSpan, readText, writeText, exists, mkdirp, removeTree,
  copyFile, writeFencedRuleset, withoutFence, copySkills, pruneSkills,
};
