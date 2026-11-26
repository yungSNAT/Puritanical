#!/usr/bin/env node
/* global Buffer, process, require -- Node.js runtime */
// puritanical — UserPromptSubmit hook
// Handles /puritanical commands, natural language activation/deactivation, and
// per-turn reinforcement.

const fs = require('fs');
const path = require('path');
const { getClaudeDir, safeWriteFlag, readFlag, resolveSections } = require('./puritanical-config');
const { requestedMode } = require('./puritanical-mode');

// Per-turn reinforcement clauses. `Commits, PRs and security warnings: project conventions, full detail.` is
// the Boundaries carve-out (always-on in SKILL.md) and ships regardless of
// `sections`; the prose/code clauses honor it the same way the SessionStart
// ruleset does.
const PROSE_CLAUSE = 'Prose: plain sentences, answer first, no preamble or recap. Keep facts and caveats.';
const CODE_CLAUSE = 'Code: YAGNI ladder first (reuse → stdlib → native → dep → one line → min code).';
const BOUNDARIES_CLAUSE = 'Commits, PRs and security warnings: project conventions, full detail.';

function reinforcementLine(sections) {
  const prose = sections ? sections.prose !== false : true;
  const code = sections ? sections.code !== false : true;
  const parts = ['PURITANICAL ACTIVE.'];
  if (prose) parts.push(PROSE_CLAUSE);
  if (code) parts.push(CODE_CLAUSE);
  parts.push(BOUNDARIES_CLAUSE);
  return parts.join(' ');
}

// Resume nudge. The prompt cache lives about an hour; after a longer break the
// next request re-writes the whole context, and every later turn re-reads it.
// On the maintainer's sessions those re-writes were 14% of spend, and
// compacting at such breaks (never mid-task) simulated to roughly half the
// total. A hook cannot run /compact, so it tells the user, once, at the break.
const RESUME_IDLE_MS = 60 * 60 * 1000;
const RESUME_MIN_CONTEXT = 150000;
const TAIL_BYTES = 512 * 1024;
// No-follow where supported, so a link swapped in after isTranscript is refused; O_NONBLOCK avoids waiting on a FIFO.
const TRANSCRIPT_FLAGS = fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0) | (fs.constants.O_NONBLOCK || 0);

// transcript_path from the hook payload must name a regular .jsonl file, not a
// link, inside Claude Code's transcript tree (<claude dir>/projects) once links
// in its folders are resolved. A FIFO or device would block the synchronous
// read below; anything else is not a transcript this hook should read.
function isTranscript(p, claudeDir) {
  if (typeof p !== 'string' || path.extname(p) !== '.jsonl') return false;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- transcript_path from Claude Code's hook payload, inspected without following a final link.
    if (!fs.lstatSync(p).isFile()) return false; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Resolves the Claude config directory's projects folder to compare real paths.
    const root = fs.realpathSync(path.join(claudeDir, 'projects')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Resolves links in the transcript's folders so a linked folder cannot leave the tree.
    const rel = path.relative(root, fs.realpathSync(p)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    return rel !== '' && rel !== '..' && !rel.startsWith('..' + path.sep) && !path.isAbsolute(rel);
  } catch { return false; }
}

function resumeWarning(transcriptPath, now = Date.now()) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- transcript_path checked by isTranscript as a regular .jsonl file under the Claude projects folder, opened read-only.
    const fd = fs.openSync(transcriptPath, TRANSCRIPT_FLAGS); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    const size = fs.fstatSync(fd).size;
    const len = Math.min(size, TAIL_BYTES);
    const buf = Buffer.alloc(len);
    fs.readSync(fd, buf, 0, len, size - len);
    fs.closeSync(fd);
    const lines = buf.toString('utf8').split('\n');
    for (let i = lines.length - 1; i >= (len === size ? 0 : 1); i--) { // line 0 of a partial tail is cut
      let e; try { e = JSON.parse(lines.at(i)); } catch { continue; }
      const u = e && e.type === 'assistant' && e.message && e.message.model !== '<synthetic>' && e.message.usage;
      if (!u) continue;
      const context = (u.input_tokens || 0) + (u.cache_creation_input_tokens || 0) + (u.cache_read_input_tokens || 0);
      const idle = now - Date.parse(e.timestamp);
      if (!(idle > RESUME_IDLE_MS) || context < RESUME_MIN_CONTEXT) return null;
      const mins = Math.round(idle / 60000);
      const gap = mins < 120 ? `${mins} min` : mins < 2880 ? `${Math.round(mins / 60)} h` : `${Math.round(mins / 1440)} days`;
      return `puritanical: ${gap} idle, so the prompt cache has expired. This turn re-sends the whole ` +
        `~${Math.round(context / 1000)}k-token context and every later turn re-reads it. ` +
        'Run /compact to continue from a summary, or /clear if this is a new task.';
    }
  } catch {}
  return null;
}

const claudeDir = getClaudeDir();
const flagPath = path.join(claudeDir, '.puritanical-active');

let input = '';
process.stdin.on('data', chunk => { input += chunk; });
process.stdin.on('end', () => {
  try {
    const data = JSON.parse(input.replace(/^\uFEFF/, ''));
    const prompt = (data.prompt || '').trim();
    const request = requestedMode(prompt);

    if (request === 'off') {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- The fixed .puritanical-active flag in the Claude config directory.
      try { fs.unlinkSync(flagPath); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    } else if (request === 'on') {
      safeWriteFlag(flagPath, 'on');
    }

    // Per-turn reinforcement
    const activeMode = readFlag(flagPath);
    if (activeMode && activeMode !== 'off') {
      // Inject compact reminder — keeps puritanical visible across context compression
      const out = {
        hookSpecificOutput: {
          hookEventName: 'UserPromptSubmit',
          additionalContext: reinforcementLine(resolveSections({ cwd: process.cwd(), claudeDir }))
        }
      };
      const warning = isTranscript(data.transcript_path, claudeDir) ? resumeWarning(data.transcript_path) : null;
      if (warning) out.systemMessage = warning;
      process.stdout.write(JSON.stringify(out));
    }
  } catch {
    // Silent fail
  }
});
