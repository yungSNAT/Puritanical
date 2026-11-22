/* global module, process, require -- Node.js runtime */
// chisle installer: the savings ledger reader and the post-install star prompt.

'use strict';

const fs = require('fs');
const path = require('path');
const cp = require('child_process');
const { getConfigDir } = require('../../hooks/chisle-config');
const { hasCmd, claudeDir } = require('./env');

// The compressor's savings ledger, or null when absent/unreadable.
function readStats(dir) {
  const p = path.join(dir, '.chisle-compress-stats.json'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Fixed ledger name in the Claude, OpenCode state or Copilot directory.
    if (fs.lstatSync(p).isSymbolicLink()) return null; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Same ledger, checked above not to be a symlink.
    const parsed = JSON.parse(fs.readFileSync(p, 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    return parsed && Number.isFinite(parsed.savedChars) && Number.isFinite(parsed.events) ? parsed : null;
  } catch { return null; }
}

// ── star prompt ─────────────────────────────────────────────────────────────
// Opt-in: asked only in an interactive terminal (not CI, not a dry run).
// Default is "later": Enter (or an answer we can't read) saves nothing and
// asks at the next install or update. Only an explicit yes or no is saved in
// the Chisle config, after which it is never asked again. Nothing touches GitHub without a
// "y"; on yes the user's own `gh` login does the starring, Chisle never sees
// a token. Without `gh` (or if it fails) it prints the links instead.
const REPO_URL = 'https://github.com/JayPokale/Chisle';
const OWNER = 'JayPokale';
const FOLLOW_URL = `https://github.com/${OWNER}`;

function starPromptState() {
  const file = path.join(getConfigDir(), 'config.json');
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- config.json in the Chisle config directory.
  if (!fs.existsSync(file)) return { file, cfg: {} }; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- config.json in the Chisle config directory.
    const cfg = JSON.parse(fs.readFileSync(file, 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    return cfg && typeof cfg === 'object' && !Array.isArray(cfg) ? { file, cfg } : null;
  } catch { return null; } // unreadable config: never overwrite it
}

function saveAnswer(st, yes) {
  if (!st) return;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The Chisle config directory.
    fs.mkdirSync(path.dirname(st.file), { recursive: true }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- config.json in the Chisle config directory.
    fs.writeFileSync(st.file, JSON.stringify({ ...st.cfg, starPrompt: yes ? 'yes' : 'no' }, null, 2) + '\n'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  } catch {}
}

function canPrompt(opts) {
  return !opts.dryRun && !process.env.CI && process.stdin.isTTY && process.stdout.isTTY;
}

// Lead with what Chisle already did for this user when the ledger says so
// (an update); a first install has nothing to show yet.
function starHeadline(opts, c) {
  const stats = readStats(claudeDir(opts));
  const tokens = stats ? Math.floor(stats.savedChars / 4) : 0;
  if (tokens < 1000) return `${c.orange('★')} Chisle is free and open source. A star helps other developers find it.`;
  const short = tokens >= 1000000 ? `${(tokens / 1000000).toFixed(1)}M` : `${Math.round(tokens / 1000)}k`;
  return `${c.orange('★')} Chisle has kept ~${short} tokens of tool output out of your context so far.`;
}

async function ask(c) {
  const rl = require('readline').createInterface({ input: process.stdin, output: process.stdout });
  const answer = await new Promise(resolve => {
    rl.on('close', () => resolve(''));
    rl.question(`  Star and follow on GitHub? ${c.dim('[y]es / [n]o / [L]ater')} `, resolve);
  });
  rl.close();
  return answer.trim().toLowerCase();
}

// Star and follow are separate calls with separate outcomes: gh's default
// login has the repo scope (star works) but not user:follow, and nobody can
// follow themselves. Report each one honestly, with a link for what failed.
function starAndFollow({ say, note, c }) {
  // The user's own gh, found on their PATH by design (Sonar S4036).
  const gh = (...args) => cp.spawnSync('gh', args, { encoding: 'utf8', timeout: 15000 }); // NOSONAR
  const hasGh = hasCmd('gh');
  const starred = hasGh && gh('api', '-X', 'PUT', `user/starred/${OWNER}/Chisle`, '--silent').status === 0;
  const me = hasGh ? (gh('api', 'user', '--jq', '.login').stdout || '').trim() : '';
  const self = me.toLowerCase() === OWNER.toLowerCase();
  const followed = !self && hasGh && gh('api', '-X', 'PUT', `user/following/${OWNER}`, '--silent').status === 0;
  if (starred) say(`  ${c.orange('★')} Starred${followed ? ' and following' : ''}. Thank you!`);
  else note(`  Couldn't star through gh. Star it here: ${REPO_URL}`);
  if (followed || self) return;
  note(hasGh && starred
    ? `  Following needs gh's user:follow scope (gh auth refresh -s user:follow). Or follow here: ${FOLLOW_URL}`
    : `  Follow here: ${FOLLOW_URL}`);
}

async function askStar(ctx) {
  const { say, note, opts, c } = ctx;
  const st = starPromptState();
  if (st && st.cfg.starPrompt) return;
  if (!canPrompt(opts)) {
    note(`If Chisle earns its keep, a star helps others find it: ${REPO_URL}`);
    return;
  }
  say('');
  say(starHeadline(opts, c));
  note(`  Yes stars ${REPO_URL.replace('https://', '')} and follows @JayPokale, using your gh login.`);
  const a = await ask(c);
  const yes = a === 'y' || a === 'yes';
  if (!yes && a !== 'n' && a !== 'no') {
    note("  Okay, we'll ask again next time you install or update.");
    return;
  }
  saveAnswer(st, yes);
  if (yes) starAndFollow(ctx);
  else note("  No problem, we won't ask again.");
}

module.exports = { readStats, askStar };
