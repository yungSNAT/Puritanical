#!/usr/bin/env node
/* global __dirname, process, require -- Node.js runtime */
// MCP proxy (hooks/chisle-mcp-proxy.js) end to end against a fake stdio
// server, plus `npx chisle mcp wrap|unwrap|status` and Copilot CLI install,
// all inside temp dirs. Run: node --test tests/test_mcp_proxy.js

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const { spawnSync, execFileSync } = require('child_process');
const isolateWorkingDirectory = require('./fixtures/working-directory');

const ROOT = path.join(__dirname, '..');
const PROXY = path.join(ROOT, 'hooks', 'chisle-mcp-proxy.js');
const SERVER = path.join(__dirname, 'fixtures', 'fake-mcp-server.js');
const CLI = path.join(ROOT, 'bin', 'install.js');

const rpc = (id, method, params) => JSON.stringify({ jsonrpc: '2.0', id, method, params });
isolateWorkingDirectory();

function proxy(lines, env) {
  const r = spawnSync(process.execPath, [PROXY, '--', process.execPath, SERVER], {
    input: lines.join('\n') + '\n', encoding: 'utf8', timeout: 20000,
    env: { ...process.env, CHISLE_DEFAULT_MODE: 'on', CHISLE_COMPRESS: '', ...env },
  });
  const messages = r.stdout.split('\n').filter(Boolean).map(line => ({ message: JSON.parse(line), line }));
  const byId = Object.fromEntries(messages.map(({ message }) => [message.id, message]));
  const raw = Object.fromEntries(messages.map(({ message, line }) => [message.id, line]));
  return { status: r.status, byId, raw };
}

function direct(lines) {
  const r = spawnSync(process.execPath, [SERVER], { input: lines.join('\n') + '\n', encoding: 'utf8' });
  return Object.fromEntries(r.stdout.split('\n').filter(Boolean).map(line => [JSON.parse(line).id, line]));
}

const LINES = [
  rpc(1, 'initialize', {}),
  rpc(2, 'tools/list'),
  rpc('a', 'tools/call', { name: 'run_logs', arguments: {} }),
  rpc(4, 'tools/call', { name: 'read_file', arguments: {} }),
  rpc(5, 'tools/call', { name: 'run_logs_fail', arguments: {} }),
];

test('proxy compresses tools/call text, salvages the mid-log error, spills the original', () => {
  const state = process.cwd();
  const { status, byId } = proxy(LINES, { CHISLE_STATE_DIR: state });
  assert.equal(status, 0);
  const text = byId.a.result.content[0].text;
  assert.ok(text.length < 5000, 'compressed: ' + text.length);
  assert.match(text, /\[chisle:/);
  assert.match(text, /ERROR: run_logs exploded/);
  assert.equal(byId[5].result.isError, true, 'isError flag kept');
  const stats = JSON.parse(fs.readFileSync('.chisle-compress-stats.json', 'utf8'));
  assert.equal(stats.events, 2);
  const spilled = fs.readdirSync('chisle-spill');
  assert.ok(spilled.some(f => f.startsWith('mcp-run_logs-')), spilled.join(','));
});

test('proxy forwards everything that is not an allowed tool result byte-identical', () => {
  const state = process.cwd();
  const { raw } = proxy(LINES, { CHISLE_STATE_DIR: state });
  const orig = direct(LINES);
  assert.equal(raw[1], orig[1], 'initialize');
  assert.equal(raw[2], orig[2], 'tools/list is not a tool result');
  assert.equal(raw[4], orig[4], 'read_file feeds edits: never compressed');
});

test('proxy is a pure pass-through when compression is off', () => {
  const state = process.cwd();
  for (const env of [{ CHISLE_COMPRESS: '0' }, { CHISLE_DEFAULT_MODE: 'off' }]) {
    const { raw } = proxy(LINES, { CHISLE_STATE_DIR: state, ...env });
    assert.deepEqual(raw, direct(LINES), JSON.stringify(env));
  }
  assert.ok(!fs.existsSync('.chisle-compress-stats.json'));
});

test('proxy exits with the server exit code; missing server exits 127', () => {
  const state = process.cwd();
  assert.equal(proxy([rpc(1, 'initialize', {})], { CHISLE_STATE_DIR: state, FAKE_MCP_EXIT: '7' }).status, 7);
  const r = spawnSync(process.execPath, [PROXY, '--', 'chisle-no-such-binary-xyz'], { input: '', encoding: 'utf8' });
  if (process.platform === 'win32') {
    // The Windows proxy spawns through cmd.exe, which reports a missing command
    // itself (localized message, exit 1) instead of raising a spawn error.
    assert.notEqual(r.status, 0);
  } else {
    assert.equal(r.status, 127);
    assert.match(r.stderr, /cannot start/);
  }
});

test('unit: responses to unknown ids and cancelled calls stay untouched', () => {
  const { rewriteLine, observeRequest } = require('../hooks/chisle-mcp-proxy');
  const pending = new Map();
  const big = JSON.stringify({ jsonrpc: '2.0', id: 9, result: { content: [{ type: 'text', text: 'x\n'.repeat(9000) }] } });
  assert.equal(rewriteLine(big, pending), big, 'no pending call');
  observeRequest(rpc(9, 'tools/call', { name: 'logs' }), pending);
  observeRequest(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: 9 } }), pending);
  assert.equal(rewriteLine(big, pending), big, 'cancelled call');
  assert.equal(rewriteLine('not json "result"', pending), 'not json "result"');
});

// ── npx chisle mcp ──────────────────────────────────────────────────────────

function cli(args, env) {
  try {
    return { status: 0, out: execFileSync(process.execPath, [CLI, ...args], { encoding: 'utf8', env: { ...process.env, ...env } }) };
  } catch (e) { return { status: e.status, out: (e.stdout || '') + (e.stderr || '') }; }
}

function isolated() {
  fs.mkdirSync('home');
  fs.mkdirSync('config');
  const home = path.resolve('home');
  const xdg = path.resolve('config');
  return { xdg, env: { CHISLE_HOME: home, XDG_CONFIG_HOME: xdg, APPDATA: xdg, COPILOT_HOME: '', CODEX_HOME: '', CLAUDE_CONFIG_DIR: path.join(home, '.claude') } };
}

test('mcp wrap routes stdio servers through the proxy; unwrap restores them exactly', () => {
  const { xdg, env } = isolated();
  const original = {
    mcpServers: {
      gh: { command: 'npx', args: ['-y', '@modelcontextprotocol/server-github'], env: { TOKEN: 'x' } },
      bare: { command: 'my-server' },
      remote: { url: 'https://example.com/mcp' },
    },
    other: true,
  };
  const originalText = JSON.stringify(original, null, 2);
  fs.mkdirSync('home/.cursor', { recursive: true });
  fs.writeFileSync('home/.cursor/mcp.json', originalText);

  const dry = cli(['mcp', 'wrap', '--dry-run'], env);
  assert.match(dry.out, /would wrap gh, bare/);
  assert.deepEqual(JSON.parse(fs.readFileSync('home/.cursor/mcp.json', 'utf8')), original, 'dry run wrote');

  cli(['mcp', 'wrap'], env);
  const wrapped = JSON.parse(fs.readFileSync('home/.cursor/mcp.json', 'utf8')).mcpServers;
  const proxyPath = path.join(xdg, 'chisle', 'mcp-proxy', 'chisle-mcp-proxy.js');
  assert.deepEqual(wrapped.gh, { command: 'node', args: [proxyPath, '--', 'npx', '-y', '@modelcontextprotocol/server-github'], env: { TOKEN: 'x' } });
  assert.deepEqual(wrapped.bare.args, [proxyPath, '--', 'my-server']);
  assert.deepEqual(wrapped.remote, original.mcpServers.remote, 'url server untouched');
  assert.ok(fs.existsSync('config/chisle/mcp-proxy/chisle-mcp-proxy.js'));
  assert.ok(fs.existsSync('config/chisle/mcp-proxy/chisle-compress-output.js'));
  // The installed copy must load on its own, without the repository's hooks/.
  const load = spawnSync(process.execPath, ['-e', 'require(process.argv[1])', path.resolve(proxyPath)], { encoding: 'utf8' });
  assert.equal(load.status, 0, load.stderr);
  assert.equal(fs.readFileSync('home/.cursor/mcp.json.chisle-bak', 'utf8'), originalText);

  assert.match(cli(['mcp', 'wrap'], env).out, /nothing to wrap/, 'idempotent');
  assert.match(cli(['mcp', 'status'], env).out, /Cursor: 2\/2 stdio server\(s\) wrapped/);

  cli(['mcp', 'unwrap'], env);
  assert.deepEqual(JSON.parse(fs.readFileSync('home/.cursor/mcp.json', 'utf8')), original);
  assert.equal(fs.readFileSync('home/.cursor/mcp.json.chisle-bak', 'utf8'), originalText);
});

test('mcp wrap leaves JSONC files alone and never touches Claude config', () => {
  const { env } = isolated();
  const jsonc = '{\n  // mine\n  "mcpServers": { "a": { "command": "x" } }\n}\n';
  fs.mkdirSync('home/.gemini', { recursive: true });
  fs.writeFileSync('home/.gemini/settings.json', jsonc);
  const claudeTxt = JSON.stringify({ mcpServers: { a: { command: 'x' } } });
  fs.writeFileSync('home/.claude.json', claudeTxt);

  assert.match(cli(['mcp', 'wrap'], env).out, /not strict JSON/);
  assert.equal(fs.readFileSync('home/.gemini/settings.json', 'utf8'), jsonc);
  assert.equal(fs.readFileSync('home/.claude.json', 'utf8'), claudeTxt);
});

test('uninstall dry-run reports restoring wrapped MCP servers', () => {
  const { env } = isolated();
  fs.mkdirSync('home/.kiro/settings', { recursive: true });
  fs.writeFileSync('home/.kiro/settings/mcp.json', JSON.stringify({ mcpServers: { a: { command: 'x' } } }));
  cli(['mcp', 'wrap'], env);
  const before = fs.readFileSync('home/.kiro/settings/mcp.json', 'utf8');
  const r = cli(['--uninstall', '--dry-run', '--no-color'], { ...env, PATH: path.dirname(process.execPath) });
  assert.match(r.out, /restored 1 MCP server command/);
  assert.equal(fs.readFileSync('home/.kiro/settings/mcp.json', 'utf8'), before, 'dry run wrote');
});

// ── Copilot CLI ─────────────────────────────────────────────────────────────

test('copilot-cli install writes ruleset + postToolUse hook; uninstall removes only ours', () => {
  const { env } = isolated();
  const root = path.resolve('home/.copilot');
  fs.mkdirSync('home/.copilot');
  fs.writeFileSync('home/.copilot/copilot-instructions.md', 'my rules\n');

  const r = cli(['--only', 'copilot-cli', '--no-color'], env);
  assert.match(r.out, /Copilot CLI detected/);
  const md = fs.readFileSync('home/.copilot/copilot-instructions.md', 'utf8');
  assert.ok(md.startsWith('my rules\n') && md.includes('<!-- chisle-begin -->'));
  const hook = JSON.parse(fs.readFileSync('home/.copilot/hooks/chisle.json', 'utf8'));
  const cmd = hook.hooks.postToolUse[0].bash;
  assert.equal(hook.version, 1);
  assert.match(cmd, /chisle-hooks[\\/]chisle-compress-output\.js"$/);

  // The installed hook answers a real Copilot payload with modifiedResult.
  const text = Array.from({ length: 3000 }, (_, i) => 'line ' + i).join('\n') + '\n<shellId: 0 completed with exit code 3>';
  const out = execFileSync(process.execPath, [path.join(root, 'chisle-hooks', 'chisle-compress-output.js')], {
    input: JSON.stringify({ sessionId: 's', toolName: 'bash', toolArgs: {}, toolResult: { resultType: 'success', textResultForLlm: text } }),
    encoding: 'utf8', env: { ...process.env, ...env, COPILOT_HOME: root, CHISLE_DEFAULT_MODE: '' },
  });
  const res = JSON.parse(out).modifiedResult;
  assert.ok(res.textResultForLlm.length < text.length / 3);
  assert.match(res.textResultForLlm, /exit code 3>$/);

  assert.match(cli(['--only', 'copilot-cli'], env).out, /already/, 'idempotent');

  cli(['--uninstall', '--only', 'copilot-cli'], env);
  assert.equal(fs.readFileSync('home/.copilot/copilot-instructions.md', 'utf8'), 'my rules\n');
  assert.ok(!fs.existsSync('home/.copilot/hooks/chisle.json'));
  assert.ok(!fs.existsSync('home/.copilot/chisle-hooks'));
});

test('copilot hook follows CHISLE_DEFAULT_MODE=off', () => {
  const text = 'x\n'.repeat(9000);
  const out = execFileSync(process.execPath, [path.join(ROOT, 'hooks', 'chisle-compress-output.js')], {
    input: JSON.stringify({ sessionId: 's', toolName: 'bash', toolResult: { resultType: 'success', textResultForLlm: text } }),
    encoding: 'utf8', env: { ...process.env, CHISLE_DEFAULT_MODE: 'off', COPILOT_HOME: process.cwd() },
  });
  assert.equal(out, '');
});

test('mcp wrap covers Codex TOML: only command/args lines change, unwrap restores the values', () => {
  const { xdg, env } = isolated();
  const original = [
    'model = "gpt-5"  # keep me',
    '',
    '[mcp_servers.context7]',
    'command = "npx"',
    'args = [',
    '  "-y",',
    '  "@upstash/context7-mcp", # pinned',
    ']',
    'env = { KEY = "x" }',
    '',
    '[mcp_servers."my server"]',
    "command = 'my-server'",
    '',
    '[mcp_servers.__proto__]',
    'command = "local-server"',
    '',
    '[mcp_servers.remote]',
    'url = "https://example.com/mcp"',
    '',
    '[profiles.fast]',
    'model = "gpt-5-mini"',
    '',
  ].join('\n');
  fs.mkdirSync('home/.codex');
  fs.writeFileSync('home/.codex/config.toml', original);

  assert.match(cli(['mcp', 'wrap', '--only', 'codex', '--dry-run'], env).out, /would wrap context7, my server, __proto__/);
  assert.equal(fs.readFileSync('home/.codex/config.toml', 'utf8'), original, 'dry run wrote');

  cli(['mcp', 'wrap', '--only', 'codex'], env);
  const proxyPath = path.join(xdg, 'chisle', 'mcp-proxy', 'chisle-mcp-proxy.js');
  const wrapped = fs.readFileSync('home/.codex/config.toml', 'utf8');
  assert.ok(wrapped.includes(`command = "node"\nargs = [${JSON.stringify(proxyPath)}, "--", "npx", "-y", "@upstash/context7-mcp"]\nenv = { KEY = "x" }`));
  assert.ok(wrapped.includes(`args = [${JSON.stringify(proxyPath)}, "--", "my-server"]`));
  assert.ok(wrapped.includes('url = "https://example.com/mcp"') && wrapped.includes('model = "gpt-5"  # keep me') && wrapped.includes('[profiles.fast]'));
  assert.match(cli(['mcp', 'status', '--only', 'codex'], env).out, /Codex: 3\/3 stdio server\(s\) wrapped/);

  cli(['mcp', 'unwrap', '--only', 'codex'], env);
  const restored = fs.readFileSync('home/.codex/config.toml', 'utf8');
  assert.ok(restored.includes('[mcp_servers.context7]\ncommand = "npx"\nargs = ["-y", "@upstash/context7-mcp"]\nenv = { KEY = "x" }'));
  assert.ok(restored.includes('[mcp_servers."my server"]\ncommand = "my-server"'));
  assert.ok(restored.includes('[mcp_servers.__proto__]\ncommand = "local-server"\n\n[mcp_servers.remote]'));
});

test('mcp wrap covers Antigravity mcp_config.json and VS Code Copilot mcp.json ("servers")', () => {
  const { xdg, env } = isolated();
  const agOrig = { mcpServers: { fs: { command: 'npx', args: ['-y', 'server-fs'] } } };
  const vsOrig = { servers: { mem: { type: 'stdio', command: 'npx', args: ['-y', 'server-memory'] }, web: { type: 'http', url: 'https://x/mcp' } }, inputs: [] };
  fs.mkdirSync('home/.gemini/config', { recursive: true });
  fs.mkdirSync('config/Code/User', { recursive: true });
  fs.writeFileSync('home/.gemini/config/mcp_config.json', JSON.stringify(agOrig, null, 2));
  fs.writeFileSync('config/Code/User/mcp.json', JSON.stringify(vsOrig, null, 2));

  const out = cli(['mcp', 'wrap'], env).out;
  assert.match(out, /Antigravity: wrapped fs/);
  assert.match(out, /GitHub Copilot: wrapped mem/);
  const proxyPath = path.join(xdg, 'chisle', 'mcp-proxy', 'chisle-mcp-proxy.js');
  const wrapped = JSON.parse(fs.readFileSync('config/Code/User/mcp.json', 'utf8'));
  assert.deepEqual(wrapped.servers.mem.args, [proxyPath, '--', 'npx', '-y', 'server-memory']);
  assert.deepEqual(wrapped.servers.web, vsOrig.servers.web, 'http server untouched');

  cli(['mcp', 'unwrap'], env);
  assert.deepEqual(JSON.parse(fs.readFileSync('home/.gemini/config/mcp_config.json', 'utf8')), agOrig);
  assert.deepEqual(JSON.parse(fs.readFileSync('config/Code/User/mcp.json', 'utf8')), vsOrig);
});

test('mcp wrap replaces a planted proxy-file link without writing through it', () => {
  const { env } = isolated();
  const proxyDir = path.join('config', 'chisle', 'mcp-proxy');
  fs.mkdirSync(proxyDir, { recursive: true });
  fs.writeFileSync('sentinel.txt', 'do not touch\n');
  fs.symlinkSync(path.resolve('sentinel.txt'), path.join(proxyDir, 'chisle-mcp-proxy.js'));
  fs.symlinkSync(path.resolve('sentinel.txt'), path.join(proxyDir, 'package.json'));

  assert.equal(cli(['mcp', 'wrap'], env).status, 0);
  assert.equal(fs.readFileSync('sentinel.txt', 'utf8'), 'do not touch\n');
  for (const [file, content] of [
    ['chisle-mcp-proxy.js', fs.readFileSync(PROXY, 'utf8')],
    ['package.json', '{ "type": "commonjs" }\n'],
  ]) {
    const st = fs.lstatSync(path.join(proxyDir, file));
    assert.ok(st.isFile() && !st.isSymbolicLink(), file);
    assert.equal(fs.readFileSync(path.join(proxyDir, file), 'utf8'), content, file);
  }
});
