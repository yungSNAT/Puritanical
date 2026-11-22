#!/usr/bin/env node
/* global __dirname, AbortSignal, fetch, module, process, require -- Node.js runtime */
// puritanical — SessionStart hook
//
// 1. Writes flag at $CLAUDE_CONFIG_DIR/.puritanical-active
// 2. Resets session turn counter
// 3. Emits the puritanical ruleset as system context
// 4. Nudges user to configure statusline if missing
// 5. Major-version update notice (majors only, cached, fail-silent)

const fs = require('fs');
const path = require('path');
const { getDefaultMode, getClaudeDir, safeWriteFlag, readJsonFile, readSettingsText, legacySetting, resolveSections, filterSections,
  OLD_NAME, migrateOldState, migrateOldConfig } = require('./puritanical-config');

// ── Update-notice helpers (pure — tested directly) ─────────────────────────
// Minor/patch releases stay quiet: a nudge per major is signal, more is spam.
const UPDATE_CACHE_MS = 3 * 24 * 60 * 60 * 1000;

function majorOf(v) {
  const m = /^(\d+)\./.exec(String(v || ''));
  return m ? parseInt(m[1], 10) : null;
}

// Notice line for a major jump, else null.
function majorUpdateNotice(installed, latest) {
  const i = majorOf(installed), l = majorOf(latest);
  if (i == null || l == null || l <= i) return null;
  if (typeof latest !== 'string' || latest.length > 128 || /[^0-9A-Za-z.+-]/.test(latest)) return null;
  return '\n\nPURITANICAL UPDATE AVAILABLE: v' + latest + ' (major; you run v' + installed + '). ' +
    'Mention this to the user once: update with `claude plugin update puritanical@puritanical`, ' +
    'or rerun the installer with `npx github:yungSNAT/Puritanical --update`.';
}

function installedVersion() {
  try {
    return JSON.parse(fs.readFileSync( // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      path.join(__dirname, '..', '.claude-plugin', 'plugin.json'), 'utf8')).version;
  } catch { return null; } // standalone-hooks install → no manifest → skip
}

// The fork's latest GitHub release, cached on disk. Network capped at 1.5s
// inside the 5s hook budget. The npm name `chisle` is upstream's (formerly Chisle).
const RELEASES_URL = 'https://api.github.com/repos/yungSNAT/Puritanical/releases/latest';

async function latestVersion(cachePath) {
  const cached = readJsonFile(cachePath, 1024);
  if (cached && Date.now() - cached.checkedAt < UPDATE_CACHE_MS) return cached.latest;
  try {
    const res = await fetch(RELEASES_URL, {
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'puritanical-update-check' },
      signal: AbortSignal.timeout(1500),
    });
    if (!res.ok) return null;
    const tag = (await res.json()).tag_name;
    if (typeof tag !== 'string' || tag.length > 128) return null;
    const latest = tag.replace(/^v/, '');
    safeWriteFlag(cachePath, JSON.stringify({ checkedAt: Date.now(), latest }));
    return latest;
  } catch { return null; }
}

// SessionStart fires on startup, resume, clear and compact. Only a genuinely
// new session needs the whole ruleset: on the other three the conversation
// already carries it, and the UserPromptSubmit reminder re-states the active
// behaviour every turn anyway. Re-sending the full ruleset each time was the bulk
// of the plugin's own overhead and piled standing instructions on top of what
// the user actually asked for. Measured and reported by @enc0ded (#2).
const FULL_INJECT_SOURCES = new Set(['startup']);

function statuslineNotice(settingsPath) {
  try {
    const raw = readSettingsText(settingsPath);
    if (raw === null) return '';
    // A badge still running the pre-4.0.0 statusline script needs setting up again.
    const line = JSON.parse(raw.replace(/^\uFEFF/, '')).statusLine;
    if (line && !JSON.stringify(line).includes(OLD_NAME + '-statusline')) return '';
  } catch (error) {
    // A missing file needs setup. Leave unreadable or malformed settings alone.
    if (error.code !== 'ENOENT') return '';
  }
  const scriptPath = path.join(__dirname, 'puritanical-statusline.sh');
  const command = `bash "${scriptPath}"`;
  const snippet = '"statusLine": { "type": "command", "command": ' + JSON.stringify(command) + ' }';
  return '\n\n' +
    'STATUSLINE SETUP NEEDED: The puritanical plugin includes a statusline badge ' +
    '([PURITANICAL]) with token savings. It is not configured yet. ' +
    'To enable, add this to ' + settingsPath + ': ' + snippet + ' ' +
    'Proactively offer to set this up for the user on first interaction.';
}

// ── Hook body ───────────────────────────────────────────────────────────────
function run(source) {
  const claudeDir = getClaudeDir();
  const flagPath = path.join(claudeDir, '.puritanical-active');
  const settingsPath = path.join(claudeDir, 'settings.json');

  // Carry an older install's state and settings across first, so the mode
  // below comes from the user's own config.
  migrateOldState(claudeDir);
  migrateOldConfig();
  const mode = getDefaultMode();

  if (mode === 'off') {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The fixed .puritanical-active entry is unlinked without following its target.
    try { fs.unlinkSync(flagPath); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    process.stdout.write('OK');
    process.exit(0);
  }

  // 1. Write flag
  safeWriteFlag(flagPath, mode);

  // 2. Resumed/cleared/compacted session: reactivate, don't re-teach.
  if (!FULL_INJECT_SOURCES.has(source)) {
    process.stdout.write(
      'PURITANICAL ACTIVE (resumed). ' +
      'Ruleset already in context; see the puritanical skill if it is not.'
    );
    return;
  }

  // 3. Read SKILL.md — single source of truth for behaviour. No level
  // filtering any more: there is one mode, so the whole body ships, minus
  // whatever `sections` in config.json opts out of (prose and/or code rules;
  // config-file only, defaults to both enabled — see puritanical-config.js).
  let skillContent = '';
  try {
    skillContent = fs.readFileSync( // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
      path.join(__dirname, '..', 'skills', 'puritanical', 'SKILL.md'), 'utf8'
    );
  } catch {}

  // Yields the prose section to an active Claude Code output style, which
  // governs the same thing and would otherwise contradict it (#15/#17).
  // An explicit sections.prose in config.json still wins.
  const sections = resolveSections({ cwd: process.cwd(), claudeDir });
  let output;

  if (skillContent) {
    output = 'PURITANICAL ACTIVE\n\n' +
      filterSections(skillContent.replace(/^---[\s\S]*?---\s*/, ''), sections);
  } else {
    // Fallback ruleset when SKILL.md not found
    const fallbackBody =
      'Puritanical: maximum-efficiency dev mode. Zero-fluff prose. YAGNI-first code.\n\n' +
      '## Persistence\n\n' +
      'ACTIVE EVERY RESPONSE. Off only: "stop puritanical" / "normal mode".\n\n' +
      '## Prose\n\n' +
      'Drop articles/filler/pleasantries/hedging. Fragments OK. Technical terms exact.\n\n' +
      '## Code\n\n' +
      'Ladder: YAGNI → reuse → stdlib → native → installed dep → one line → min code.\n' +
      'No unrequested abstractions. Deletion over addition. Shortest diff wins. Check the reported and related cases. Leave correct code alone. Preserve caller contracts: report skips in best-effort imports; reject invalid atomic batches before mutation.';
    output = 'PURITANICAL ACTIVE\n\n' + filterSections(fallbackBody, sections, ['Prose'], ['Code']);
  }

  output += statuslineNotice(settingsPath);

  // 3b. One-time courtesy for the 3.0.0 upgrade: a lite/full/ultra setting is
  // no longer meaningful. Puritanical still runs (the value falls through to the
  // default), but silently ignoring a setting someone chose is worse than
  // saying so once.
  const legacy = legacySetting();
  if (legacy) {
    output += '\n\nPURITANICAL NOTE: `' + legacy.value + '` came from ' + legacy.source +
      '. Intensity levels were removed in 3.0.0 — there is one mode now, and it is ' +
      'active. Set it to `on` or `off`, or drop it entirely. Mention this once.';
  }

  // 4. Update notice, then emit. PURITANICAL_UPDATE_CHECK=0 disables.
  void (async () => {
    try {
      if (process.env.PURITANICAL_UPDATE_CHECK !== '0') {
        const installed = installedVersion();
        if (installed) {
          const latest = await latestVersion(path.join(claudeDir, '.puritanical-update-check.json'));
          const notice = majorUpdateNotice(installed, latest);
          if (notice) output += notice;
        }
      }
    } catch {}
    process.stdout.write(output);
  })();
}

// Claude Code pipes the hook payload (including `source`) on stdin. If it is
// absent or unparseable, fall back to a full inject — over-teaching once is a
// far cheaper failure than a session that never receives the ruleset at all.
function main() {
  let input = '';
  process.stdin.on('data', chunk => { input += chunk; });
  process.stdin.on('end', () => {
    let source = 'startup';
    try {
      const parsed = JSON.parse(input.replace(/^\uFEFF/, ''));
      if (parsed && typeof parsed.source === 'string') source = parsed.source;
    } catch {}
    run(source);
  });
  // stdin never opened (manual run): behave like a fresh session.
  if (process.stdin.isTTY) run('startup');
}

if (require.main === module) main();

module.exports = { majorOf, majorUpdateNotice, latestVersion };
