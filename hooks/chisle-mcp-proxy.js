#!/usr/bin/env node
/* global module, process, require -- Node.js runtime */
// chisle — stdio MCP proxy: tool-output compression for any MCP host.
//
// Most agents cannot rewrite a tool result from a hook (Windsurf, Cline,
// Kiro, Gemini, Qwen; Cursor only for MCP), but every one of them launches
// stdio MCP servers from a command line it does not inspect. Wrapping that
// command puts Chisle between host and server:
//
//   node chisle-mcp-proxy.js -- <original command> [args...]
//
// Bytes pass through untouched in both directions, except the JSON-RPC
// response to a `tools/call` request: its `content[]` text blocks go through
// the same scrub + elide core as every other harness (head/tail kept, error
// lines salvaged, full original spilled to disk). Everything else — init,
// tools/list, notifications, resources, malformed lines — is forwarded
// byte-identical. `npx chisle mcp wrap` rewrites host configs to use it.
//
// Deliberately absent:
//   - dedup markers: one server process can outlive a chat and serve the
//     next one, whose context never saw the earlier copy.
//   - read/edit-shaped tools (read_file, get_file_contents, edit_file, ...):
//     their output feeds exact-match edits. Skipped by name.
//   - structuredContent: left as-is; hosts that forward it get it unchanged.
//
// Never breaks the server: any compressor error forwards the original line.
// Tunables: CHISLE_COMPRESS=0, CHISLE_DEFAULT_MODE=off, CHISLE_COMPRESS_MAX_CHARS
// (+ HEAD/TAIL_LINES), CHISLE_COMPRESS_SPILL=0, CHISLE_STATE_DIR (spill + stats,
// default ~/.config/chisle).

'use strict';

const cp = require('child_process');
const { StringDecoder } = require('string_decoder');
const core = require('./chisle-compress-output');
const { getDefaultMode, getConfigDir } = require('./chisle-config');

const IS_WIN = process.platform === 'win32';

const MAX_PENDING_LINE = 16 * 1024 * 1024; // drop observation of absurd lines

function mcpToolAllowed(name) {
  // Tool names whose output may feed a later exact-match edit, or is file
  // content the model quotes back: the hook's shared guard, on the raw name.
  return typeof name === 'string' && !!name && !core.unsafeToolName(name);
}

function stateDir() {
  return process.env.CHISLE_STATE_DIR || getConfigDir();
}

// Compress one tools/call result in place. Returns chars saved (0 = untouched).
function compressResult(result, toolName) {
  if (!result || !Array.isArray(result.content)) return 0;
  const limits = core.limitsFor();
  let saved = 0;
  for (const block of result.content) {
    if (!block || block.type !== 'text' || typeof block.text !== 'string') continue;
    if (block.text.includes('[chisle:')) continue; // already compressed upstream
    const updated = core.transform(block.text, limits, 'mcp-' + toolName, stateDir());
    if (updated != null && updated.length < block.text.length) {
      saved += block.text.length - updated.length;
      block.text = updated;
    }
  }
  return saved;
}

// One server→host line. Returns the line to forward (original when untouched).
function rewriteLine(line, pending) {
  if (!pending.size || line.indexOf('"result"') === -1) return line;
  let msg;
  try { msg = JSON.parse(line); } catch { return line; }
  const msgs = Array.isArray(msg) ? msg : [msg];
  let saved = 0;
  for (const m of msgs) {
    if (!m || m.id == null || !('result' in m)) continue;
    const key = JSON.stringify(m.id);
    const tool = pending.get(key);
    if (tool === undefined) continue;
    pending.delete(key);
    if (!mcpToolAllowed(tool)) continue;
    saved += compressResult(m.result, tool);
  }
  if (!saved) return line;
  core.recordSavings(saved, stateDir());
  return JSON.stringify(msg);
}

// One host→server line: remember which request ids are tool calls.
function observeRequest(line, pending) {
  if (line.indexOf('tools/call') === -1 && line.indexOf('notifications/cancelled') === -1) return;
  let msg;
  try { msg = JSON.parse(line); } catch { return; }
  for (const m of Array.isArray(msg) ? msg : [msg]) {
    if (m && m.method === 'tools/call' && m.id != null) {
      pending.set(JSON.stringify(m.id), m.params && typeof m.params.name === 'string' ? m.params.name : '');
    }
    // A cancelled call never gets a result; forget it.
    if (m && m.method === 'notifications/cancelled' && m.params && m.params.requestId != null) {
      pending.delete(JSON.stringify(m.params.requestId));
    }
  }
}

// Splits a byte stream into lines, calling onLine(lineWithoutNewline, eol).
function lineSplitter(onLine) {
  let buf = '';
  return {
    push(chunk) {
      buf += chunk;
      let i;
      while ((i = buf.indexOf('\n')) !== -1) {
        const raw = buf.slice(0, i);
        buf = buf.slice(i + 1);
        const cr = raw.endsWith('\r');
        onLine(cr ? raw.slice(0, -1) : raw, cr ? '\r\n' : '\n');
      }
    },
    rest() { const r = buf; buf = ''; return r; },
    get size() { return buf.length; },
  };
}

// Windows argv quoting in one pass: inside the quotes, n backslashes before a
// quote become 2n+1 plus the quote, n at the end become 2n, others stay.
function quoteWin(a) {
  if (a !== '' && !/[\s"&|<>^()%!]/.test(a)) return a;
  let out = '"';
  let run = 0;
  for (const ch of String(a)) {
    if (ch === '\\') { run++; continue; }
    out += '\\'.repeat(ch === '"' ? 2 * run + 1 : run) + ch;
    run = 0;
  }
  return out + '\\'.repeat(2 * run) + '"';
}

// Stream error handlers. Writes to a server that has exited fail with EPIPE;
// the child's close handler reports its exit status, so that error is dropped.
function ignoreServerGone() {
  return function serverGone() {};
}

// The host stopped reading: stop the server and exit quietly.
function exitWhenHostGone(child) {
  return function hostGone() {
    try { child.kill(); } catch {}
    process.exit(0);
  };
}

function main(argv) {
  const sep = argv.indexOf('--');
  const cmd = sep === -1 ? argv : argv.slice(sep + 1);
  if (!cmd.length) {
    process.stderr.write('usage: chisle-mcp-proxy -- <command> [args...]\n');
    process.exit(2);
  }

  // Windows MCP commands are usually npx/uvx shims (.cmd), which Node refuses
  // to spawn without a shell; elsewhere spawn directly, no shell parsing.
  // The proxy exists to run the MCP server command from the user's own agent
  // configuration, so the command is trusted; quoteWin quotes each argument.
  const child = IS_WIN
    // eslint-disable-next-line security-node/detect-child-process -- .cmd shims need cmd.exe; the command comes from the user's MCP config.
    ? cp.spawn(cmd.map(quoteWin).join(' '), [], { stdio: ['pipe', 'pipe', 'inherit'], shell: true, windowsHide: true }) // nosemgrep: javascript_exec_rule-child-process, javascript.lang.security.audit.spawn-shell-true.spawn-shell-true, javascript.lang.security.detect-child-process.detect-child-process
    : cp.spawn(cmd[0], cmd.slice(1), { stdio: ['pipe', 'pipe', 'inherit'] }); // nosemgrep: javascript_exec_rule-child-process, javascript.lang.security.detect-child-process.detect-child-process

  const enabled = process.env.CHISLE_COMPRESS !== '0' && getDefaultMode() !== 'off';
  const pending = new Map();

  child.on('error', (e) => {
    process.stderr.write('chisle-mcp-proxy: cannot start ' + cmd[0] + ': ' + e.message + '\n');
    process.exit(127);
  });
  child.stdin.on('error', ignoreServerGone());
  process.stdout.on('error', exitWhenHostGone(child));

  // Host → server: forward raw, observe tool calls on the side.
  const reqLines = lineSplitter(line => observeRequest(line, pending));
  const decoder = new StringDecoder('utf8');
  process.stdin.on('data', (chunk) => {
    child.stdin.write(chunk);
    if (!enabled) return;
    reqLines.push(decoder.write(chunk));
    if (reqLines.size > MAX_PENDING_LINE) reqLines.rest();
  });
  process.stdin.on('end', () => child.stdin.end());

  // Server → host: rewrite only tool-call responses. Paused while the host is
  // slow to read so a chatty server cannot balloon this process.
  if (!enabled) {
    child.stdout.pipe(process.stdout);
  } else {
    const resLines = lineSplitter((line, eol) => {
      let out = line;
      try { out = rewriteLine(line, pending); } catch { out = line; }
      if (!process.stdout.write(out + eol)) {
        child.stdout.pause();
        process.stdout.once('drain', () => child.stdout.resume());
      }
    });
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', chunk => resLines.push(chunk));
    child.stdout.on('end', () => { const r = resLines.rest(); if (r) process.stdout.write(r); });
  }

  for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(sig, () => { try { child.kill(sig); } catch {} });
  }

  child.on('close', (code, signal) => {
    const status = code != null ? code : (signal ? 128 + (new Map(Object.entries(require('os').constants.signals)).get(signal) || 1) : 1);
    process.stdout.write('', () => process.exit(status));
  });
}

if (require.main === module) main(process.argv.slice(2));

module.exports = { mcpToolAllowed, compressResult, rewriteLine, observeRequest, lineSplitter, exitWhenHostGone, quoteWin };
