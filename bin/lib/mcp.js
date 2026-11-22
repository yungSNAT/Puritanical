/* global __dirname, module, process, require -- Node.js runtime */
// puritanical — `npx github:yungSNAT/Puritanical mcp <wrap|unwrap|status>`: route stdio MCP servers
// through hooks/puritanical-mcp-proxy.js so their oversized tool results get the
// same compression Claude Code and Pi get from PostToolUse.
//
// Opt-in, because it rewrites other tools' config files:
//   - strict JSON only: a file with comments is reported and left alone,
//     since rewriting it would drop them
//   - first write of each file keeps a `<file>.puritanical-bak` copy
//   - only stdio entries (`command`) are wrapped; url/http servers untouched
//   - wrapped form is reversible from the args alone, no extra keys a host
//     schema might reject: { command: "node", args: [proxy, "--", cmd, ...args] }
//
// Claude Code is never a target: its PostToolUse hook already compresses
// mcp__* output, and wrapping would only add a process per server.
// Codex (TOML config) is not wrapped automatically.

'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const { getConfigDir, readSettingsText, secureDirectory } = require('../../hooks/puritanical-config');
const { writeAtomically } = require('./settings');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const PROXY_FILE = 'puritanical-mcp-proxy.js';
const PROXY_SOURCES = ['puritanical-mcp-proxy.js', 'puritanical-compress-output.js', 'puritanical-config.js', 'puritanical-command-filters.js'];

function home() { return process.env.PURITANICAL_HOME || os.homedir(); }
function proxyDir() { return path.join(getConfigDir(), 'mcp-proxy'); }

// VS Code-family user dirs, for Cline's global MCP settings.
function vscodeUserDirs() {
  const base = process.platform === 'darwin' ? path.join(home(), 'Library', 'Application Support')
    : process.platform === 'win32' ? (process.env.APPDATA || path.join(home(), 'AppData', 'Roaming'))
    : (process.env.XDG_CONFIG_HOME || path.join(home(), '.config'));
  return ['Code', 'Code - Insiders', 'VSCodium', 'Cursor', 'Windsurf'].map(n => path.join(base, n, 'User')); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
}

// Global MCP config files. JSON ones hold `{ "mcpServers": { name: { command, args } } }`
// unless the target names another key (VS Code: "servers"); Codex keeps
// `[mcp_servers.<name>]` tables in TOML.
function targets() {
  const h = home();
  const copilot = process.env.COPILOT_HOME || path.join(h, '.copilot');
  const codex = process.env.CODEX_HOME || path.join(h, '.codex');
  const list = [
    ['cursor', 'Cursor', path.join(h, '.cursor', 'mcp.json')],
    ['windsurf', 'Windsurf', path.join(h, '.codeium', 'windsurf', 'mcp_config.json')],
    ['gemini', 'Gemini CLI', path.join(h, '.gemini', 'settings.json')],
    ['qwen', 'Qwen Code', path.join(h, '.qwen', 'settings.json')],
    ['kiro', 'Kiro', path.join(h, '.kiro', 'settings', 'mcp.json')],
    ['copilot-cli', 'Copilot CLI', path.join(copilot, 'mcp-config.json')],
    ['antigravity', 'Antigravity', path.join(h, '.gemini', 'config', 'mcp_config.json')],
    ['codex', 'Codex', path.join(codex, 'config.toml'), { format: 'toml' }],
  ];
  // Copilot in VS Code: the user-profile mcp.json, keyed "servers".
  for (const dir of vscodeUserDirs().slice(0, 3)) list.push(['copilot', 'GitHub Copilot', path.join(dir, 'mcp.json'), { key: 'servers' }]);
  for (const dir of vscodeUserDirs()) {
    list.push(['cline', 'Cline', path.join(dir, 'globalStorage', 'saoudrizwan.claude-dev', 'settings', 'cline_mcp_settings.json')]);
  }
  return list.map(([id, label, file, extra]) => ({ id, label, file, ...extra }));
}

function isWrapped(entry) {
  return !!(entry && Array.isArray(entry.args) && typeof entry.args[0] === 'string' &&
    path.basename(entry.args[0]) === PROXY_FILE && entry.args[1] === '--');
}

function isStdio(entry) {
  return !!(entry && typeof entry.command === 'string' && entry.command &&
    !entry.url && !entry.httpUrl && !entry.serverUrl &&
    (entry.type == null || entry.type === 'stdio' || entry.type === 'local'));
}

function wrapEntry(entry, proxy) {
  const args = Array.isArray(entry.args) ? entry.args : [];
  entry.args = [proxy, '--', entry.command, ...args];
  entry.command = 'node';
}

function unwrapEntry(entry) {
  const rest = entry.args.slice(2);
  entry.command = rest[0];
  entry.args = rest.slice(1);
  if (!entry.args.length) delete entry.args; // wrap added it; most configs omit an empty one
}

function loadConfigText(file) {
  try {
    const raw = readSettingsText(file);
    return raw === null ? { error: 'not a readable, bounded config file' } : { raw };
  } catch (error) {
    if (error.code === 'ENOENT') return { missing: true };
    return { error: 'could not read config (' + error.code + ')' };
  }
}

function readConfig(file) {
  const loaded = loadConfigText(file);
  if (loaded.raw === undefined) return loaded;
  const { raw } = loaded;
  if (!raw.trim()) return { data: {}, raw };   // an empty file has no servers yet
  try {
    const data = JSON.parse(raw.replace(/^\uFEFF/, ''));
    if (!data || typeof data !== 'object' || Array.isArray(data)) return { error: 'not a JSON object' };
    return { data, raw };
  } catch { return { error: 'not strict JSON (comments?); edit by hand' }; }
}

function writeConfig(file, raw, data) {
  writeText(file, raw, JSON.stringify(data, null, 2) + '\n');
}

function writeText(file, raw, text) {
  const bak = file + '.puritanical-bak';
  try {
    writeAtomically(bak, raw, { noReplace: true });
  } catch (error) {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- The .puritanical-bak backup beside an MCP config file from the target table or the user's own --file argument.
    if (error.code !== 'EEXIST' || !fs.lstatSync(bak).isFile()) throw error; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  }
  let mode = 0o600;
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- An MCP config file from the target table or the user's own --file argument.
  try { mode = fs.statSync(file).mode & 0o777; } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  writeAtomically(file, text, { mode });
}

// ── Codex TOML ──────────────────────────────────────────────────────────────
// Only the command/args lines of [mcp_servers.<name>] tables are read and
// rewritten; every other byte of config.toml is left as it was. A table whose
// values are not plain strings / string arrays is skipped, never guessed at.
const TOML_TABLE = /^\s*\[\s*mcp_servers\.(?:"((?:[^"\\]|\\.)*)"|([A-Za-z0-9_-]+))\s*\]\s*(?:#.*$|$)/;

// Parse TOML strings from text starting at index i: one string, or an array
// of strings when the text opens with '['. Returns { value, end } or null.
function tomlValue(text, i) {
  const str = () => {
    const q = text.charAt(i);
    if (text.startsWith(q.repeat(3), i)) return null;   // multi-line strings: not handled
    let j = i + 1;
    while (j < text.length && text.charAt(j) !== q) {
      if (q === '"' && text.charAt(j) === '\\') j++;
      j++;
    }
    if (j >= text.length) return null;
    const lit = text.slice(i, j + 1);
    i = j + 1;
    try { return q === "'" ? lit.slice(1, -1) : JSON.parse(lit); } catch { return null; }
  };
  const skip = () => {
    while (i < text.length) {
      const char = text.charAt(i);
      if (/\s|,/.test(char)) i++;
      else if (char === '#') {
        const newline = text.indexOf('\n', i);
        i = newline < 0 ? text.length : newline;
      } else break;
    }
  };
  skip();
  const first = text.charAt(i);
  if (first === '"' || first === "'") {
    const value = str();
    return value == null ? null : { value, end: i };
  }
  if (first !== '[') return null;
  i++;
  const arr = [];
  for (;;) {
    skip();
    const char = text.charAt(i);
    if (char === ']') return { value: arr, end: i + 1 };
    if (char !== '"' && char !== "'") return null;
    const v = str();
    if (v == null) return null;
    arr.push(v);
  }
}

function readToml(file) {
  const loaded = loadConfigText(file);
  if (loaded.raw === undefined) return loaded;
  const { raw } = loaded;
  const lines = raw.split('\n');
  const tables = new Map();
  let cur = null;
  for (let n = 0; n < lines.length; n++) {
    const line = lines.at(n);
    const m = TOML_TABLE.exec(line);
    if (m) {
      let name = m[2];
      if (m[1] !== undefined) {
        try { name = JSON.parse('"' + m[1] + '"'); }
        catch { cur = null; continue; }
      }
      cur = { entry: {}, ok: true };
      tables.set(name, cur);
      continue;
    }
    if (/^\s*\[/.test(line)) { cur = null; continue; }
    const kv = cur && /^(\s*)(command|args|url)\s*=\s*/.exec(line);
    if (!kv) continue;
    // An array may run over several lines: parse from here and find its end.
    const rest = lines.slice(n).join('\n');
    const v = kv[2] === 'url' ? { value: 'url', end: line.length } : tomlValue(rest, kv[0].length);
    if (!v || (kv[2] === 'command' && typeof v.value !== 'string') || (kv[2] === 'args' && !Array.isArray(v.value))) { cur.ok = false; continue; }
    const last = n + (rest.slice(0, v.end).match(/\n/g) || []).length;
    cur.entry[kv[2]] = v.value;
    cur[kv[2]] = { start: n, end: last, indent: kv[1] };
    n = last;
  }
  const servers = new Map();
  for (const [name, table] of tables) {
    if (table.ok) servers.set(name, table.entry);
  }
  return { raw, lines, tables, servers };
}

function writeToml(file, cfg, names) {
  const lines = cfg.lines.slice();
  const edits = [];
  for (const name of names) {
    const t = cfg.tables.get(name), e = t.entry;
    const indent = t.command ? t.command.indent : '';
    const argsLine = e.args && e.args.length ? [indent + 'args = [' + e.args.map(a => JSON.stringify(a)).join(', ') + ']'] : [];
    edits.push({ start: t.command.start, end: t.command.end, text: [indent + 'command = ' + JSON.stringify(e.command)].concat(t.args ? [] : argsLine) });
    if (t.args) edits.push({ start: t.args.start, end: t.args.end, text: argsLine });
  }
  for (const ed of edits.sort((a, b) => b.start - a.start)) lines.splice(ed.start, ed.end - ed.start + 1, ...ed.text);
  writeText(file, cfg.raw, lines.join('\n'));
}

// Removes a proxy file before it is written again. A link is removed itself,
// never followed, so a planted link cannot redirect the write outside the folder.
function clearProxyEntry(file) {
  let st;
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- A fixed proxy file name in the checked mcp-proxy folder, inspected without following links.
  try { st = fs.lstatSync(file); } catch (e) { if (e.code === 'ENOENT') return; throw e; } // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  if (!st.isFile() && !st.isSymbolicLink()) throw new Error(file + ' is not a regular file');
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Removes the entry itself (a file or a link, not its target) in the checked mcp-proxy folder.
  fs.unlinkSync(file); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
}

function installProxy() {
  const dir = proxyDir();
  if (!secureDirectory(dir, getConfigDir())) throw new Error('unsafe MCP proxy folder (a link or not a folder): ' + dir);
  for (const f of PROXY_SOURCES) {
    const dst = path.join(dir, f);
    clearProxyEntry(dst);
    // Exclusive create: fails rather than follow a link planted after the removal.
    fs.copyFileSync(path.join(REPO_ROOT, 'hooks', f), dst, fs.constants.COPYFILE_EXCL); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  }
  const pkg = path.join(dir, 'package.json');
  clearProxyEntry(pkg);
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Exclusive create of package.json in the checked mcp-proxy folder.
  fs.writeFileSync(pkg, '{ "type": "commonjs" }\n', { flag: 'wx' }); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  return path.join(dir, PROXY_FILE);
}

// Proxies copied by an older version can lack newer sources
// (puritanical-command-filters.js), so an install or update refreshes an existing
// copy. Returns its folder, or null when no proxy is installed.
function refreshProxy() {
  const dir = proxyDir();
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- The mcp-proxy folder inside the Puritanical config directory.
  if (!fs.existsSync(dir)) return null; // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  installProxy();
  return dir;
}

function parse(argv) {
  const o = { action: argv[0], dryRun: false, only: [], files: [] };
  const args = argv.slice(1);
  while (args.length) {
    const a = args.shift();
    if (a === '--dry-run') o.dryRun = true;
    else if (a === '--only' && args[0]) o.only.push(args.shift());
    else if (a === '--file' && args[0]) o.files.push(path.resolve(args.shift())); // nosemgrep: javascript.lang.security.audit.path-traversal.path-join-resolve-traversal.path-join-resolve-traversal
    else if (a === '--no-color' || a === '--') continue;
    else throw new Error('unknown option: ' + a);
  }
  if (!['wrap', 'unwrap', 'status'].includes(o.action)) throw new Error('usage: npx github:yungSNAT/Puritanical mcp <wrap|unwrap|status> [--only <agent>] [--file <mcp.json>] [--dry-run]');
  return o;
}

function selected(o) {
  if (o.files.length) return o.files.map(file => ({ id: 'file', label: 'custom', file, ...(file.endsWith('.toml') ? { format: 'toml' } : {}) }));
  return targets().filter(t => !o.only.length || o.only.includes(t.id));
}

// The servers table of a loaded config → { key, entries }.
function serverEntries(t, cfg) {
  const key = t.format === 'toml' ? 'mcp_servers' : t.key || (cfg.data.mcpServers ? 'mcpServers' : 'servers');
  return { key, entries: cfg.servers ?? Object.getOwnPropertyDescriptor(cfg.data, key)?.value };
}

// Wrap or unwrap `todo` in one file and save it (unless --dry-run).
function applyChanges(t, cfg, o, proxy, todo, out) {
  for (const [, entry] of todo) {
    if (o.action === 'wrap') wrapEntry(entry, proxy);
    else unwrapEntry(entry);
  }
  const names = todo.map(([name]) => name);
  if (!o.dryRun) (t.format === 'toml' ? writeToml(t.file, cfg, names) : writeConfig(t.file, cfg.raw, cfg.data));
  out('  ' + t.label + ': ' + (o.dryRun ? 'would ' + o.action : o.action + 'ped') + ' ' + names.join(', ') + '  ' + t.file);
  return todo.length;
}

// One config file: report, wrap or unwrap. Servers changed, or null when the file is missing.
function runTarget(t, o, proxy, out) {
  const cfg = t.format === 'toml' ? readToml(t.file) : readConfig(t.file);
  if (cfg.missing) { if (o.files.length) out('  ' + t.file + ': not found'); return null; }
  if (cfg.error) { out('  ' + t.label + ': ' + t.file + ': skipped, ' + cfg.error); return 0; }
  const { key, entries } = serverEntries(t, cfg);
  if (!entries || typeof entries !== 'object') { out('  ' + t.label + ': no ' + key + ' in ' + t.file); return 0; }

  const servers = entries instanceof Map ? [...entries] : Object.entries(entries);
  const stdio = servers.filter(([, entry]) => isStdio(entry));
  const wrapped = servers.filter(([, entry]) => isWrapped(entry));
  if (o.action === 'status') {
    out('  ' + t.label + ': ' + wrapped.length + '/' + stdio.length +
      ' stdio server(s) wrapped  ' + t.file);
    return 0;
  }

  const todo = o.action === 'wrap'
    ? stdio.filter(([, entry]) => !isWrapped(entry))
    : wrapped;
  if (!todo.length) { out('  ' + t.label + ': nothing to ' + o.action + '  ' + t.file); return 0; }
  return applyChanges(t, cfg, o, proxy, todo, out);
}

function printSavings(out) {
  const dir = process.env.PURITANICAL_STATE_DIR || getConfigDir();
  let stats = null;
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- Fixed ledger name in PURITANICAL_STATE_DIR or the Puritanical config directory.
  try { stats = JSON.parse(fs.readFileSync(path.join(dir, '.puritanical-compress-stats.json'), 'utf8')); } catch {} // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  out('\n  saved so far: ' + (stats && Number.isFinite(stats.savedChars)
    ? stats.savedChars.toLocaleString('en-US') + ' chars over ' + stats.events + ' tool result(s)'
    : 'nothing yet') + '  (' + dir + ')');
}

// Returns { changed, files } so `--uninstall` can reuse it quietly.
function run(argv, out) {
  out = out || (s => process.stdout.write(s + '\n'));
  const o = parse(argv);
  const proxy = o.action === 'wrap' && !o.dryRun ? installProxy() : path.join(proxyDir(), PROXY_FILE);
  let changed = 0, found = 0;

  for (const t of selected(o)) {
    const n = runTarget(t, o, proxy, out);
    if (n === null) continue;
    found++;
    changed += n;
  }

  if (!found) out('  no MCP config files found' + (o.only.length ? ' for ' + o.only.join(', ') : ''));
  if (o.action === 'status') printSavings(out);
  if (o.action === 'wrap' && changed && !o.dryRun) out('\n  Restart the agent so it relaunches its MCP servers.');
  return { changed, found };
}

function main(argv) {
  try { run(argv); }
  catch (e) { process.stderr.write('error: ' + e.message + '\n'); process.exit(2); }
}

module.exports = { main, run, targets, isWrapped, isStdio, proxyDir, refreshProxy };
