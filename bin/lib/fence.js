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
const { NAME, OLD_NAME } = require('../../hooks/puritanical-config');

const FENCE_BEGIN = '<!-- puritanical-begin -->';
const FENCE_END = '<!-- puritanical-end -->';
// The markers an install before 4.0.0 wrote. Their block is replaced, never
// duplicated, and uninstall removes it too.
const OLD_MARKERS = ['<!-- ' + OLD_NAME + '-begin -->', '<!-- ' + OLD_NAME + '-end -->'];

// The first block, current markers first: b and e are the offsets of its
// begin and end markers (e is -1 when the end marker is missing), end is the
// offset just past the end marker, and old marks a block with the old markers.
function fenceSpan(txt) {
  for (const [begin, endMarker] of [[FENCE_BEGIN, FENCE_END], OLD_MARKERS]) {
    const b = txt.indexOf(begin);
    if (b === -1) continue;
    const e = txt.indexOf(endMarker, b);
    return { b, e, end: e === -1 ? -1 : e + endMarker.length, old: begin !== FENCE_BEGIN };
  }
  return { b: -1, e: -1, end: -1, old: false };
}

// Drops a complete block with the old markers from a file that also holds a
// current block, so the old rules never load beside the new ones.
function withoutOldFence(txt) {
  if (!txt.includes(FENCE_BEGIN)) return txt;
  const b = txt.indexOf(OLD_MARKERS[0]);
  const e = b === -1 ? -1 : txt.indexOf(OLD_MARKERS[1], b);
  if (e === -1) return txt;
  return txt.slice(0, b) + txt.slice(e + OLD_MARKERS[1].length).replace(/^\n/, '');
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

function noteOldFenceBackup(target, txt, note) {
  if (!txt.includes(OLD_MARKERS[0])) return;
  const base = target + '.' + OLD_NAME + '-bak';
  for (let n = 1; ; n++) {
    const dst = n === 1 ? base : base + '-' + n;
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- Collision-safe backup alongside the target instructions file.
      fs.writeFileSync(dst, txt, { flag: 'wx', mode: 0o644 }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      note('  backed up pre-4.0.0 instructions to ' + dst);
      return;
    } catch (e) { if (e && e.code !== 'EEXIST') return; }
  }
}

// Fenced AGENTS.md append/refresh for Codex-style targets. Appends after
// existing content (never prepends, never clobbers), refreshes one fenced
// block in place under --force. Returns 'installed', 'refreshed', 'skipped'.
function writeFencedRuleset(target, opts, note) {
  const raw = readText(path.join(REPO_ROOT, 'AGENTS.md'));
  const block = FENCE_BEGIN + '\n' + raw.trimEnd() + '\n' + FENCE_END + '\n';
  mkdirp(path.dirname(target));
  let found = '';
  try { found = readText(target); } catch (err) {
    if (err.code !== 'ENOENT') throw err;
  }
  const existing = withoutOldFence(found);
  const { b, e, end, old } = fenceSpan(existing);
  if (b === -1) {
    writeText(target, existing + separator(existing) + block);
    process.stdout.write('  installed: ' + target + '\n');
    return 'installed';
  }
  if (!opts.force && !old) {
    if (existing !== found) {
      noteOldFenceBackup(target, found, note);
      writeText(target, existing);
      process.stdout.write('  removed the pre-4.0.0 ruleset from ' + target + '\n');
    }
    note('  ' + target + ' already contains puritanical ruleset (--force to refresh)');
    return 'skipped';
  }
  // A begin marker with no end marker means the file was hand-edited or a
  // write was cut short. Everything after the begin marker is then user
  // content of unknown extent, so refuse rather than guess: rewriting would
  // delete the rest of their global instructions.
  if (e === -1) {
    note('  ' + target + ' has a puritanical-begin marker with no puritanical-end; leaving it alone');
    note('  remove the stray marker by hand, then re-run');
    return 'skipped';
  }
  noteOldFenceBackup(target, found, note);
  let tail = existing.slice(end);
  if (tail.charAt(0) === '\n') tail = tail.slice(1);
  writeText(target, existing.slice(0, b) + block + tail);
  process.stdout.write('  ' + (old ? 'replaced the pre-4.0.0 ruleset' : 'refreshed ruleset') + ' in ' + target + '\n');
  return old ? 'installed' : 'refreshed';
}

function separator(existing) {
  if (!existing || existing.slice(-2) === '\n\n') return '';
  return existing.slice(-1) === '\n' ? '\n' : '\n\n';
}

// Text with the puritanical block cut out, or null when there is no complete block.
function withoutFence(txt) {
  const { b, e, end } = fenceSpan(txt);
  if (e === -1) return null;
  const tail = txt.slice(end).replace(/^\n/, '');
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

// The same skills under the names an install before 4.0.0 used.
function oldSkillNames() {
  return ownedSkillNames().map(name => name.replace(NAME, OLD_NAME));
}

// Puritanical-owned skill dirs present under dstRoot, by either name.
function presentSkills(dstRoot, names) {
  return names.filter(name => exists(path.join(dstRoot, name))); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

// Removes only the skill dirs Puritanical owns (by default under both names);
// foreign skills stay put. Returns the count removed (or that would be
// removed on a dry run).
function pruneSkills(dstRoot, dryRun, names = ownedSkillNames().concat(oldSkillNames())) {
  const present = presentSkills(dstRoot, names);
  if (!dryRun) for (const name of present) removeTree(path.join(dstRoot, name)); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return present.length;
}

module.exports = {
  FENCE_BEGIN, FENCE_END, fenceSpan, readText, writeText, exists, mkdirp, removeTree,
  copyFile, writeFencedRuleset, withoutFence, withoutOldFence, copySkills, pruneSkills, oldSkillNames,
};
