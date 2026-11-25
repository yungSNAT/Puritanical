/* global module, process, require -- Node.js runtime */
'use strict';
// `npx github:yungSNAT/Puritanical tools [trim|restore] [--days N] [--also A,B] [--dry-run] [--config-dir <dir>]`
//
// Every Claude Code request carries fixed overhead the model may never use:
//   - the full schema of every loaded tool. On the maintainer's machine
//     `Artifact` alone was ~54k chars, with SendFeedback, Workflow and
//     ScheduleWakeup another ~26k, none called in 38 sessions. A whole-tool
//     `permissions.deny` entry drops a schema (measured: 24.2k -> 20.5k tokens
//     for five tools).
//   - the skill listing, a name plus a description per skill (~8-14k chars).
//     `skillOverrides: { name: "name-only" }` keeps a skill listed and
//     invocable by name but drops its description.
//
//   tools          report sizes vs. your own usage; changes nothing
//   tools trim     deny unused heavy tools and list unused skills name-only,
//                  judged over --days (default 30); --also A,B adds rarely used
//                  tools you choose (never core tools)
//   tools restore  undo exactly what `trim` changed
//
// Sizes come from the prompt snapshots and skill listings Claude Code writes
// into transcripts; usage from the tool and Skill calls in the same
// transcripts. Core tools are never trimmed, and neither are MCP tools (Claude
// Code already defers those).

const fs = require('fs');
const path = require('path');
const SETTINGS = require('./settings');

const CORE = new Set(['Bash', 'Read', 'Edit', 'MultiEdit', 'Write', 'Glob', 'Grep', 'NotebookEdit',
  'Agent', 'Task', 'TodoWrite', 'ToolSearch', 'Skill', 'WebFetch', 'WebSearch', 'AskUserQuestion',
  'EnterPlanMode', 'ExitPlanMode', 'Monitor', 'TaskStop', 'TaskCreate', 'TaskGet', 'TaskList',
  'TaskUpdate', 'SendMessage', 'ListAgents', 'LSP']);
const MIN_CHARS = 2000;        // smaller schemas are not worth a deny entry
const MIN_SKILL_CHARS = 150;   // shorter descriptions are not worth an override
const CHARS_PER_TOKEN = 5.5;   // measured on tool schemas: 5 denied tools, -3,707 tokens
const RECORD = '.puritanical-trimmed-tools.json';
const DAY_MS = 24 * 60 * 60 * 1000;

function transcripts(dir, since) {
  const out = [];
  const walk = (d) => {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Directories under the Claude config directory's projects/ transcript tree.
    let entries; try { entries = fs.readdirSync(d, { withFileTypes: true }); } catch { return; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    for (const e of entries) {
      const p = path.join(d, e.name); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
      if (e.isDirectory()) walk(p);
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- A .jsonl transcript found under the Claude config directory's projects/ tree.
      else if (e.name.endsWith('.jsonl')) { try { if (fs.statSync(p).mtimeMs >= since) out.push(p); } catch {} } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    }
  };
  walk(path.join(dir, 'projects')); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  return out;
}

// Largest schema / description seen per tool and skill, calls of each, and the
// request count, over the window.
const bump = (map, key, n = 1) => map.set(key, (map.get(key) ?? 0) + n);
const keepMax = (map, key, n) => map.set(key, Math.max(map.get(key) ?? 0, n));

function parseLine(line) {
  try { return JSON.parse(line); } catch { return null; }
}

// Tool schemas sent with one request.
function scanSnapshot(e, acc) {
  const tools = e.attachment && Array.isArray(e.attachment.tools) ? e.attachment.tools : [];
  for (const t of tools) {
    if (!t || typeof t.name !== 'string' || !t.name) continue;
    keepMax(acc.size, t.name, JSON.stringify(t).length);
  }
}

// One skill listing: `- name: description` items.
function scanListing(e, acc) {
  const listing = e.attachment && typeof e.attachment.content === 'string' ? e.attachment.content : '';
  const entries = [];
  for (const item of listing.split(/\n(?=- )/)) {
    const k = item.indexOf(': ');           // names may contain ':' (plugin:skill)
    if (!item.startsWith('- ') || k < 0) continue;
    const name = item.slice(2, k);
    keepMax(acc.skillSize, name, item.length - k - 2);
    entries.push([name, item.length - k - 2]);
  }
  if (entries.length) acc.listings.push(entries);
}

// An assistant message: one request per message id, plus its tool and Skill calls.
function scanAssistant(e, acc, seen) {
  const m = e.message;
  if (!m || e.type !== 'assistant') return;
  if (m.usage && m.id && !seen.has(m.id)) { seen.add(m.id); acc.requests++; }
  for (const b of Array.isArray(m.content) ? m.content : []) {
    if (b?.type !== 'tool_use') continue;
    if (typeof b.name === 'string') bump(acc.calls, b.name);
    const s = b.name === 'Skill' && b.input && b.input.skill;
    if (typeof s === 'string' && s) bump(acc.skillCalls, s);
  }
}

// Which scanner a transcript line needs, judged by substring before parsing.
function lineScanner(line) {
  if (line.includes('"prompt_snapshot"')) return scanSnapshot;
  if (line.includes('"skill_listing"')) return scanListing;
  if (line.includes('"tool_use"') || line.includes('"usage"')) return scanAssistant;
  return null;
}

function scan(files) {
  const acc = { size: new Map(), calls: new Map(), skillSize: new Map(), skillCalls: new Map(), listings: [], requests: 0 };
  for (const f of files) {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Transcripts listed from the Claude config directory's projects/ tree.
    let text; try { text = fs.readFileSync(f, 'utf8'); } catch { continue; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    const seen = new Set();
    for (const line of text.split('\n')) {
      const scanner = lineScanner(line);
      const e = scanner && parseLine(line);
      if (e) scanner(e, acc, seen);
    }
  }
  return acc;
}

function plan(claudeDir, days) {
  const files = transcripts(claudeDir, Date.now() - days * DAY_MS);
  const { size, calls, skillSize, skillCalls, listings, requests } = scan(files);
  const rows = [...size]
    .filter(([name, chars]) => chars >= MIN_CHARS && !name.startsWith('mcp__'))
    .map(([name, chars]) => ({ name, chars, tokens: Math.round(chars / CHARS_PER_TOKEN), calls: calls.get(name) ?? 0,
      trim: !CORE.has(name) && !calls.has(name) }))
    .sort((a, b) => b.chars - a.chars);
  const skills = [...skillSize]
    .filter(([, chars]) => chars >= MIN_SKILL_CHARS)
    .map(([name, chars]) => ({ name, chars, tokens: Math.round(chars / 4), calls: skillCalls.get(name) ?? 0, trim: !skillCalls.has(name) }))
    .sort((a, b) => b.chars - a.chars);
  // A session lists only its own project's skills, so the per-request saving
  // is the average over real listings, not the sum of every skill ever seen.
  const unused = new Set(skills.filter((x) => x.trim).map((x) => x.name));
  const skillSaving = listings.length ? Math.round(listings.reduce((t, l) => t + l.reduce((u, [n, c]) => u + (unused.has(n) ? c : 0), 0), 0) / listings.length / 4) : 0;
  return { rows, skills, skillSaving, sessions: files.length, requests };
}

function readRecord(claudeDir) {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The fixed trim record in the Claude config directory.
    const r = JSON.parse(fs.readFileSync(path.join(claudeDir, RECORD), 'utf8')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename, javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    return { added: r.added || [], skills: r.skills || [] };
  } catch { return { added: [], skills: [] }; }
}

function load(claudeDir) {
  const settingsPath = path.join(claudeDir, 'settings.json'); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  const settings = SETTINGS.readSettings(settingsPath);
  if (!settings) throw new Error(`${settingsPath} is not valid JSON; not touching it`);
  return { settingsPath, settings };
}

// Undo exactly what `trim` changed; the user's own deny rules and skill
// overrides are left alone.
function restore(claudeDir, { dryRun = false } = {}) {
  const rec = readRecord(claudeDir);
  if (!rec.added.length && !rec.skills.length) return [];
  const { settingsPath, settings } = load(claudeDir);
  const deny = (settings.permissions && settings.permissions.deny) || [];
  const overrides = settings.skillOverrides || {};
  const restoredSkills = rec.skills.filter((name) =>
    Object.getOwnPropertyDescriptor(overrides, name)?.value === 'name-only');
  const removed = rec.added.filter((n) => deny.includes(n))
    .concat(restoredSkills.map((name) => `skill ${name}`));
  if (!dryRun) {
    if (settings.permissions) {
      settings.permissions.deny = deny.filter((n) => !rec.added.includes(n));
      if (!settings.permissions.deny.length) delete settings.permissions.deny;
      if (!Object.keys(settings.permissions).length) delete settings.permissions;
    }
    if (settings.skillOverrides) {
      const restored = new Set(restoredSkills);
      const preserved = Object.entries(overrides).filter(([name]) => !restored.has(name));
      if (preserved.length) settings.skillOverrides = Object.fromEntries(preserved);
      else delete settings.skillOverrides;
    }
    SETTINGS.writeSettings(settingsPath, settings);
    fs.rmSync(path.join(claudeDir, RECORD), { force: true }); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  }
  return removed;
}

function trim(claudeDir, names, skillNames = [], { dryRun = false } = {}) {
  const { settingsPath, settings } = load(claudeDir);
  const deny = (settings.permissions && settings.permissions.deny) || [];
  const overrides = settings.skillOverrides || {};
  const fresh = names.filter((n) => !deny.includes(n));
  const freshSkills = skillNames.filter((name) => !Object.hasOwn(overrides, name));
  if (!dryRun && (fresh.length || freshSkills.length)) {
    if (fresh.length) {
      settings.permissions = settings.permissions || {};
      settings.permissions.deny = deny.concat(fresh);
    }
    if (freshSkills.length) {
      settings.skillOverrides = Object.fromEntries([
        ...Object.entries(overrides),
        ...freshSkills.map((name) => [name, 'name-only']),
      ]);
    }
    SETTINGS.writeSettings(settingsPath, settings);
    const rec = readRecord(claudeDir);
    const next = { added: [...new Set(rec.added.concat(fresh))], skills: [...new Set(rec.skills.concat(freshSkills))] };
    SETTINGS.writeSettings(path.join(claudeDir, RECORD), next); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
  }
  return { tools: fresh, skills: freshSkills };
}

const USAGE = 'usage: npx github:yungSNAT/Puritanical tools [trim|restore] [--days N] [--also A,B] [--dry-run] [--config-dir <dir>]';

function parseArgs(argv) {
  const o = { action: 'report', days: 30, dryRun: false, also: [] };
  const args = argv.slice();
  while (args.length) {
    const a = args.shift();
    if (a === 'trim' || a === 'restore') o.action = a;
    else if (a === '--dry-run') o.dryRun = true;
    else if (a === '--days') o.days = Number(args.shift());
    else if (a === '--also') o.also = String(args.shift() || '').split(',').map((x) => x.trim()).filter(Boolean);
    else throw new Error(USAGE);
  }
  const core = o.also.filter((n) => CORE.has(n));
  if (core.length) throw new Error(`refusing to deny core tool(s): ${core.join(', ')}`);
  if (!(o.days > 0)) throw new Error('--days needs a positive number');
  return o;
}

function toolVerdict(r) {
  if (r.trim) return 'unused → deny';
  return CORE.has(r.name) ? 'core' : 'used';
}

function printUsage(out, rows, skills) {
  for (const r of rows) out(`  ${r.name.padEnd(30)} ~${String(r.tokens).padStart(6)} tok   ${String(r.calls).padStart(5)} calls   ${toolVerdict(r)}`);
  if (skills.length) out('');
  for (const s of skills) out(`  skill ${s.name.padEnd(24)} ~${String(s.tokens).padStart(6)} tok   ${String(s.calls).padStart(5)} calls   ${s.trim ? 'unused → name-only' : 'used'}`);
}

function main(argv, claudeDir, out = (s) => process.stdout.write(s + '\n')) {
  const o = parseArgs(argv);

  if (o.action === 'restore') {
    const removed = restore(claudeDir, o);
    out(removed.length ? `  ${o.dryRun ? 'would undo' : 'undid'}: ${removed.join(', ')}` : '  nothing to restore');
    return;
  }

  const { rows, skills, skillSaving, sessions, requests } = plan(claudeDir, o.days);
  if (!rows.length && !skills.length) {
    out(`  no tool schemas or skill listings found in ${sessions} session(s) from the last ${o.days} days (needs a recent Claude Code).`);
    return;
  }
  out(`  sent on every request vs. your calls, last ${o.days} days (${sessions} sessions, ${requests} requests):\n`);
  printUsage(out, rows, skills);
  for (const r of rows) if (o.also.includes(r.name)) r.trim = true;
  const pick = rows.filter((r) => r.trim), pickSkills = skills.filter((s) => s.trim);
  const perRequest = pick.reduce((s, r) => s + r.tokens, 0) + skillSaving;
  out(`\n  trimming ${pick.length} tool(s) and ${pickSkills.length} skill description(s) saves ~${perRequest.toLocaleString('en-US')} tokens on every request` +
    (requests ? ` (~${(perRequest * requests / 1e6).toFixed(1)}M over these ${requests} requests).` : '.'));

  if (o.action === 'trim') trimAndReport(claudeDir, pick, pickSkills, o, out);
  else if (pick.length || pickSkills.length) {
    out(`  apply with: npx github:yungSNAT/Puritanical tools trim${o.also.length ? ' --also ' + o.also.join(',') : ''}   (undo: npx github:yungSNAT/Puritanical tools restore)`);
  }
}

function trimAndReport(claudeDir, pick, pickSkills, o, out) {
  const done = trim(claudeDir, pick.map((r) => r.name), pickSkills.map((s) => s.name), o);
  const what = [];
  if (done.tools.length) what.push(`denied ${done.tools.join(', ')}`);
  if (done.skills.length) what.push(`listed ${done.skills.length} skill(s) name-only`);
  out(what.length ? `  ${o.dryRun ? 'would have ' : ''}${what.join('; ')}. Undo: npx github:yungSNAT/Puritanical tools restore` : '  nothing new to trim.');
}

module.exports = { main, plan, trim, restore, CORE };
