#!/usr/bin/env node
/* global process, require -- Node.js runtime */
// Tool-output compressor tests — correctness guardrails and never-crash.
// Run: node --test tests/test_compress.js

const { test } = require('node:test');
const assert = require('node:assert/strict');
const {
  exists, listDir, makeTempDir, readText, remove, under, writeText,
} = require('./helpers/sandbox');

const {
  extractText, rebuildResponse, scrub, compress, transform, limitsFor, toolAllowed, processPayload, THRESHOLDS,
  unsafeToolName, containsCredential,
} = require('../hooks/chisle-compress-output');

const FULL = THRESHOLDS;

function bigOutput(lines, prefix) {
  return Array.from({ length: lines }, (_, i) => `${prefix || 'line'} ${i} ${'x'.repeat(80)}`).join('\n');
}

// ── extractText ──────────────────────────────────────────────────────────────

test('extractText handles string, object, and array shapes', () => {
  assert.equal(extractText('plain'), 'plain');
  assert.equal(extractText({ stdout: 'out', stderr: 'err' }), 'out\nerr');
  assert.equal(extractText([{ type: 'text', text: 'a' }, { type: 'text', text: 'b' }]), 'a\nb');
  assert.equal(extractText(null), null);
});

// ── compress ─────────────────────────────────────────────────────────────────

test('compress keeps head and tail, elides middle', () => {
  const text = bigOutput(500);
  const out = compress(text, FULL);
  assert.ok(out.length < text.length);
  assert.ok(out.startsWith('line 0 '));
  assert.ok(out.trimEnd().endsWith('x'.repeat(80)));
  assert.ok(out.includes('line 499 '));         // tail survived
  assert.ok(out.includes('[chisle: elided'));      // marker present
  assert.ok(!out.includes('line 250 '));        // middle gone
});

test('compress salvages error lines from the elided middle', () => {
  const lines = bigOutput(500).split('\n');
  lines[250] = 'FATAL ERROR: connection refused at db.js:42';
  const out = compress(lines.join('\n'), FULL);
  assert.ok(out.includes('FATAL ERROR: connection refused at db.js:42'));
  assert.ok(out.includes('error-like line(s) below'));
});

test('compress hard-cuts a single giant line by chars', () => {
  const text = 'y'.repeat(50000);
  const out = compress(text, FULL);
  assert.ok(out.length < text.length);
  assert.ok(out.includes('elided'));
});

test('compress returns null when no win', () => {
  assert.equal(compress('short', FULL), null);
});

test('shared compressor never splits surrogate pairs at head/tail cuts', () => {
  const text = 'a'.repeat(3999) + '😀' + 'b'.repeat(5000) + '😀' + 'c'.repeat(3999);
  const out = compress(text, FULL);
  assert.ok(out.startsWith('a'.repeat(3999) + '\n'));
  assert.ok(out.endsWith('\n' + 'c'.repeat(3999)));
  assert.doesNotMatch(out, /[\uD800-\uDFFF]/u);
});

test('shared compressor windows long diagnostics around the match', () => {
  const lines = bigOutput(500).split('\n');
  lines[250] = '😀'.repeat(1000) + ' FATAL LATE_ERROR ' + '😀'.repeat(1000);
  const out = compress(lines.join('\n'), FULL);
  assert.match(out, /FATAL LATE_ERROR/);
  assert.doesNotMatch(out, /[\uD800-\uDFFF]/u);
});

// ── scrub (lossless tier) ────────────────────────────────────────────────────

test('scrub strips ANSI escapes', () => {
  const out = scrub('\x1b[31mFAIL\x1b[0m test_foo\n\x1b]0;title\x07plain');
  assert.equal(out, 'FAIL test_foo\nplain');
});

test('scrub collapses identical consecutive lines', () => {
  const out = scrub(Array(10).fill('WARN: retrying connection').join('\n'));
  assert.ok(out.includes('WARN: retrying connection'));
  assert.ok(out.includes('[chisle: line repeated 10×]'));
  assert.equal(out.split('\n').length, 2);
});

test('scrub keeps short repeats and blank structure', () => {
  const src = 'a\na\na\n\nb';               // 3 repeats < threshold of 4
  assert.equal(scrub(src), src);
  assert.equal(scrub('x\n\n\n\n\ny'), 'x\n\ny');  // blank run → one blank
});

test('scrub collapses passing-test runs but keeps every failure', () => {
  const lines = [' FAIL  test/price.test.js'];
  for (let i = 0; i < 40; i++) lines.push(`    ✓ applyDiscount ${i * 100} at 10% → ${i * 90} (${i % 5} ms)`);
  lines.push('    ✕ applyDiscount rounds half-cents up: 1995 at 10% → 1796 (3 ms)');
  for (let i = 0; i < 40; i++) lines.push(`    ✓ slugs "Hello World ${i}" (2 ms)`);
  lines.push('Tests:       1 failed, 80 passed, 81 total');
  const out = scrub(lines.join('\n'));
  assert.ok(out.includes('FAIL  test/price.test.js'));
  assert.ok(out.includes('✕ applyDiscount rounds half-cents up'));
  assert.ok(out.includes('1 failed, 80 passed'));
  assert.ok(out.includes('[chisle: 37 more passing/progress lines like these]'));
  assert.ok(out.split('\n').length < 15);
});

test('scrub leaves data rows that merely look alike untouched', () => {
  const csv = Array.from({ length: 50 }, (_, i) => `2024-01-${String(i % 28 + 1).padStart(2, '0')},cust${i},${i * 13}`).join('\n');
  assert.equal(scrub(csv), csv);
});

test('scrub minifies pretty JSON without touching strings or big integers', () => {
  const src = JSON.stringify([{ id: 'X', n: 1 }], null, 2).replace('"X"', '1234567890123456789').replace('"n"', '"a  b"');
  const out = scrub(src);
  assert.equal(out, '[{"id":1234567890123456789,"a  b":1}]');
  assert.equal(scrub('{ not json\n  at all }'), '{ not json\n  at all }');
});

test('transform scrubs medium outputs below the elision threshold', () => {
  const noisy = Array(50).fill('\x1b[32m✓\x1b[0m ok').join('\n') + '\n' + 'tail '.repeat(300);
  assert.ok(noisy.length < THRESHOLDS.maxChars);
  const out = transform(noisy, THRESHOLDS);
  assert.ok(out && out.length < noisy.length);
  assert.ok(!out.includes('\x1b['));
});

test('transform returns null when the win is trivial', () => {
  const clean = 'z'.repeat(2000);            // nothing to scrub, under maxChars
  assert.equal(transform(clean, THRESHOLDS), null);
});

test('transform still elides oversized outputs after scrubbing', () => {
  const out = transform(bigOutput(500), THRESHOLDS);
  assert.ok(out.includes('[chisle: elided'));
});

// ── dedup ────────────────────────────────────────────────────────────────────

test('identical consecutive tool output becomes a marker (isolated config dir)', () => {
  const dir = require('fs').mkdtempSync(require('path').join(require('os').tmpdir(), 'chisle-dedup-'));
  const saved = process.env.CLAUDE_CONFIG_DIR;
  process.env.CLAUDE_CONFIG_DIR = dir;
  try {
    const payload = { tool_name: 'Bash', session_id: 'sess-1', tool_response: 'FAIL test_x\n' + 'ctx '.repeat(1000) };
    const first = processPayload(payload, 'on');
    const second = processPayload(payload, 'on');
    assert.ok(second && second.includes('byte-identical to the previous Bash result'));
    assert.ok(second.includes('FAIL test_x'));          // first lines kept
    assert.notDeepEqual(first, second);                  // only the repeat is deduped
    const third = processPayload({ tool_name: 'Bash', session_id: 'sess-1', tool_response: 'different '.repeat(300) }, 'on');
    assert.ok(third == null || !third.includes('byte-identical'));
    // New session must NOT dedup against the old one — the earlier copy is
    // not in the new session's context.
    const newSession = processPayload({ ...payload, session_id: 'sess-2' }, 'on');
    assert.ok(newSession == null || !newSession.includes('byte-identical'));
    // No session_id (portability unknowns) → dedup skipped entirely.
    const noSess = processPayload({ tool_name: 'Bash', tool_response: payload.tool_response }, 'on');
    assert.ok(noSess == null || !noSess.includes('byte-identical'));
  } finally {
    if (saved !== undefined) process.env.CLAUDE_CONFIG_DIR = saved;
    else delete process.env.CLAUDE_CONFIG_DIR;
    require('fs').rmSync(dir, { recursive: true, force: true });
  }
});

test('REGRESSION: two hook copies on ONE tool call must not dedup each other', () => {
  const dir = require('fs').mkdtempSync(require('path').join(require('os').tmpdir(), 'chisle-dupreg-'));
  const saved = process.env.CLAUDE_CONFIG_DIR;
  process.env.CLAUDE_CONFIG_DIR = dir;
  try {
    const call = {
      tool_name: 'Bash', session_id: 'sess-1', tool_use_id: 'toolu_01AAA',
      tool_response: 'line one never seen before\n' + 'payload '.repeat(400),
    };
    const copyA = processPayload(call, 'on');   // plugin-manifest registration
    const copyB = processPayload(call, 'on');   // settings.json registration
    assert.ok(copyA == null || !copyA.includes('byte-identical'));
    assert.ok(copyB == null || !copyB.includes('byte-identical'),
      'second registration falsely deduped a first-seen output');

    // A genuine re-run of the same command is a different tool call, so the
    // real dedup must still fire.
    const rerun = processPayload({ ...call, tool_use_id: 'toolu_02BBB' }, 'on');
    assert.ok(rerun && rerun.includes('byte-identical to the previous Bash result'));
  } finally {
    if (saved !== undefined) process.env.CLAUDE_CONFIG_DIR = saved;
    else delete process.env.CLAUDE_CONFIG_DIR;
    require('fs').rmSync(dir, { recursive: true, force: true });
  }
});

// ── limitsFor ────────────────────────────────────────────────────────────────

test('one threshold set, no levels', () => {
  assert.equal(limitsFor().maxChars, THRESHOLDS.maxChars);
  assert.equal(limitsFor().headLines, THRESHOLDS.headLines);
  assert.equal(limitsFor().tailLines, THRESHOLDS.tailLines);
});

test('env vars override thresholds', () => {
  process.env.CHISLE_COMPRESS_MAX_CHARS = '1234';
  assert.equal(limitsFor().maxChars, 1234);
  delete process.env.CHISLE_COMPRESS_MAX_CHARS;
});

// ── toolAllowed — the correctness allowlist ──────────────────────────────────

test('Read/Edit/Write are NEVER compressed (Edit old_string correctness)', () => {
  assert.equal(toolAllowed('Read'), false);
  assert.equal(toolAllowed('Edit'), false);
  assert.equal(toolAllowed('Write'), false);
  assert.equal(toolAllowed('NotebookEdit'), false);
});

test('Bash, Agent, mcp__ tools are compressible', () => {
  assert.equal(toolAllowed('Bash'), true);
  assert.equal(toolAllowed('Agent'), true);
  assert.equal(toolAllowed('mcp__context7__query-docs'), true);
  assert.equal(toolAllowed('mcp__github__list_issues'), true);
});

// File readers, LSP tools and code-host file tools return content the model
// quotes back or edits against, so the shared name guard keeps them whole,
// even when CHISLE_COMPRESS_TOOLS names them.
const UNSAFE_MCP = [
  'mcp__tokensave__tokensave_read', 'mcp__filesystem__read_text_file', 'mcp__github__get_file_contents',
  'mcp__github__search_code', 'mcp__ide__lsp_hover', 'mcp__serena__find_symbol', 'mcp__lsp__goto_definition',
  'mcp__gitlab__get_blob', 'mcp__repo__view_source', 'mcp__x__apply_patch', 'mcp__ide__hover',
  'mcp__ide__find_references', 'mcp__lsp__goto_implementation', 'mcp__lsp__goto_declaration',
];

test('MCP file, source and LSP tools are never compressed (Claude/Pi)', () => {
  for (const t of UNSAFE_MCP) assert.equal(toolAllowed(t), false, t);
  process.env.CHISLE_COMPRESS_TOOLS = UNSAFE_MCP.join(',');
  try {
    for (const t of UNSAFE_MCP) assert.equal(toolAllowed(t), false, `${t} via override`);
  } finally { delete process.env.CHISLE_COMPRESS_TOOLS; }
});

test('unsafeToolName is the one guard the proxy, Pi, Copilot and OpenCode share', () => {
  const { mcpToolAllowed } = require('../hooks/chisle-mcp-proxy');
  const { piToolAllowed } = require('../pi-extension/index.js');
  const { copilotToolAllowed, opencodeToolAllowed } = require('../hooks/chisle-compress-output');
  for (const t of ['get_file_contents', 'search_code', 'lsp_references', 'find_symbol', 'read_resource', 'hover',
    'find_references', 'goto_implementation']) {
    assert.equal(unsafeToolName(t), true, t);
    assert.equal(mcpToolAllowed(t), false, `proxy: ${t}`);
  }
  assert.equal(mcpToolAllowed('run_logs'), true);
  assert.equal(piToolAllowed('mcp__github__get_file_contents'), false);
  assert.equal(piToolAllowed('mcp__github__list_issues'), true);
  process.env.CHISLE_COMPRESS_TOOLS = 'bash,github_get_file_contents,github-tools_search_code';
  try {
    assert.equal(copilotToolAllowed('github_get_file_contents'), false);
    assert.equal(opencodeToolAllowed('github-tools_search_code'), false);
    assert.equal(opencodeToolAllowed('bash'), true);
  } finally { delete process.env.CHISLE_COMPRESS_TOOLS; }
  for (const t of ['Bash', 'Agent', 'WebFetch', 'WebSearch', 'Grep', 'Glob', 'powershell', 'web_fetch', 'task', 'shell', 'list']) {
    assert.equal(unsafeToolName(t), false, `built-in ${t} must stay compressible`);
  }
});

test('CHISLE_COMPRESS_TOOLS overrides the allowlist', () => {
  process.env.CHISLE_COMPRESS_TOOLS = 'Grep';
  assert.equal(toolAllowed('Bash'), false);
  assert.equal(toolAllowed('Grep'), true);
  assert.equal(toolAllowed('mcp__x__y'), false); // override disables mcp__ default
  delete process.env.CHISLE_COMPRESS_TOOLS;
});

// ── processPayload — full pipeline ───────────────────────────────────────────

test('processPayload compresses a big Bash output', () => {
  const out = processPayload({ tool_name: 'Bash', tool_response: { stdout: bigOutput(500) } }, 'on');
  assert.ok(out && out.includes('[chisle: elided'));
});

test('processPayload passes small outputs through (null)', () => {
  assert.equal(processPayload({ tool_name: 'Bash', tool_response: 'tiny' }, 'on'), null);
});

test('processPayload does nothing when mode is off or missing', () => {
  const payload = { tool_name: 'Bash', tool_response: bigOutput(500) };
  assert.equal(processPayload(payload, 'off'), null);
  assert.equal(processPayload(payload, null), null);
});

test('CHISLE_COMPRESS=0 is a kill switch', () => {
  process.env.CHISLE_COMPRESS = '0';
  assert.equal(processPayload({ tool_name: 'Bash', tool_response: bigOutput(500) }, 'on'), null);
  delete process.env.CHISLE_COMPRESS;
});

test('processPayload never throws on garbage', () => {
  assert.equal(processPayload(null, 'on'), null);
  assert.equal(processPayload({}, 'on'), null);
  assert.equal(processPayload({ tool_name: 'Bash' }, 'on'), null);
  assert.equal(processPayload({ tool_name: 'Bash', tool_response: 12345 }, 'on'), null);
});

// ── rebuildResponse: the replacement must match the tool's output shape ──────
// Regression guard for #3 (@sovdchains): emitting a bare string for an
// object-shaped tool made Claude Code reject every replacement, so the
// compressor silently did nothing while still logging savings.

test('REGRESSION #3: object-shaped response stays an object', () => {
  const res = { stdout: 'original', stderr: '', interrupted: false };
  const out = rebuildResponse(res, 'compressed');
  assert.equal(typeof out, 'object');
  assert.equal(out.stdout, 'compressed');
  // untouched fields survive, or the harness rejects on the schema again
  assert.equal(out.interrupted, false);
});

test('REGRESSION #3: stderr is blanked so it is not duplicated', () => {
  // extractText folds stderr into the compressed text; leaving it would repeat it
  const out = rebuildResponse({ stdout: 'a', stderr: 'boom' }, 'a\nboom');
  assert.equal(out.stdout, 'a\nboom');
  assert.equal(out.stderr, '');
});

test('string-shaped response stays a string', () => {
  assert.equal(rebuildResponse('original', 'compressed'), 'compressed');
  assert.equal(rebuildResponse(null, 'compressed'), 'compressed');
});

test('alternative string carriers are found', () => {
  assert.equal(rebuildResponse({ output: 'x' }, 'c').output, 'c');
  assert.equal(rebuildResponse({ result: 'x' }, 'c').result, 'c');
  assert.equal(rebuildResponse({ text: 'x' }, 'c').text, 'c');
});

test('unknown shapes are skipped, never guessed', () => {
  // a wrong guess is silently rejected by the harness — the bug we are fixing
  assert.equal(rebuildResponse([{ type: 'text', text: 'x' }], 'c'), null);
  assert.equal(rebuildResponse({ content: [{ text: 'x' }] }, 'c'), null);
  assert.equal(rebuildResponse({ nothing: 1 }, 'c'), null);
});

// ── recoverable elision (spill) ─────────────────────────────────────────────
// Destroying the middle made re-running the command the only way to get it
// back: more expensive than the elision saved, and wrong for anything
// non-idempotent. These cover the escape hatch that replaced that.

const fs = require('fs');

function withSpillDir(fn) {
  const dir = makeTempDir('chisle-spill-test-');
  const prev = process.env.CLAUDE_CONFIG_DIR;
  process.env.CLAUDE_CONFIG_DIR = dir;
  try { return fn(dir); }
  finally {
    if (prev === undefined) delete process.env.CLAUDE_CONFIG_DIR;
    else process.env.CLAUDE_CONFIG_DIR = prev;
    remove(dir, { recursive: true, force: true });
  }
}

test('an elided output spills in full and the marker points at it', () => {
  withSpillDir(() => {
    const text = bigOutput(500);
    const out = transform(text, FULL, 'Bash');

    const marker = out.split('\n').find(l => l.includes('chisle:'));
    const m = marker.match(/Full output: (\S+)/);
    assert.ok(m, `marker carries no path: ${marker}`);

    const spilled = readText(m[1]);
    assert.equal(spilled, text, 'spill is not the complete output');
    assert.ok(out.length < text.length, 'compression did not shrink');

    // the point of the exercise: a line from the destroyed middle is recoverable
    assert.ok(!out.includes('line 250 '), 'middle was not actually elided');
    assert.ok(spilled.includes('line 250 '), 'middle is not recoverable');

    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Inspects a file this test created under os.tmpdir().
    if (process.platform !== 'win32') assert.equal(fs.statSync(m[1]).mode & 0o777, 0o600, 'spill is not owner-only'); // NTFS has no POSIX mode bits // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  });
});

test('spilling is off for callers that pass no tool name', () => {
  withSpillDir((dir) => {
    const out = transform(bigOutput(500), FULL);
    assert.doesNotMatch(out, /Full output:/);
    assert.ok(!exists(under(dir, 'chisle-spill')), 'replay path wrote spill files');
  });
});

test('CHISLE_COMPRESS_SPILL=0 elides without the escape hatch', () => {
  withSpillDir((dir) => {
    process.env.CHISLE_COMPRESS_SPILL = '0';
    try {
      const out = transform(bigOutput(500), FULL, 'Bash');
      assert.ok(out.length < bigOutput(500).length, 'still must compress');
      assert.doesNotMatch(out, /Full output:/);
      assert.ok(!exists(under(dir, 'chisle-spill')));
    } finally { delete process.env.CHISLE_COMPRESS_SPILL; }
  });
});

test('the spill directory stays bounded', () => {
  withSpillDir((dir) => {
    for (let i = 0; i < 55; i++) transform(bigOutput(500, `run${i}`), FULL, 'Bash');
    const files = listDir(under(dir, 'chisle-spill'));
    assert.ok(files.length <= 41, `spill dir grew to ${files.length} files`);
  });
});

// Spill files outlive the session, so output carrying a credential is elided
// without being written to disk. Fixtures are assembled at run time so the
// source holds no credential-shaped literal for scanners to flag.
const CREDENTIALS = {
  'PEM private key': '-----BEGIN RSA ' + 'PRIVATE KEY-----\nMIIEow' + 'A'.repeat(40),
  'OpenSSH private key': '-----BEGIN OPENSSH ' + 'PRIVATE KEY-----',
  JWT: 'ey' + 'JhbGciOiJIUzI1NiJ9.' + 'ey' + 'JzdWIiOiIxMjM0NTY3ODkwIn0.' + 'S'.repeat(43),
  'AWS access key ID': 'AK' + 'IA' + 'Q'.repeat(16),
  'AWS secret key': 'aws_secret_access_key = ' + 'a/B+'.repeat(10),
  'GCP API key': 'AI' + 'za' + 'S'.repeat(35),
  'GCP OAuth token': 'ya' + '29.' + 'a0'.repeat(20),
  'GCP client secret': 'GOC' + 'SPX-' + 'x'.repeat(28),
  'Azure storage key': 'DefaultEndpointsProtocol=https;Account' + 'Key=' + 'Ab+/'.repeat(10),
  'Azure SAS signature': 'https://a.blob.core.windows.net/c?sv=2024&' + 'sig=' + 'AbC%2B'.repeat(6),
  'Azure client secret': 'abc' + '8Q~' + 'd'.repeat(34),
  'GitHub classic token': 'gh' + 'p_' + 'A1'.repeat(18),
  'GitHub OAuth token': 'gh' + 'o_' + 'B2'.repeat(18),
  'GitHub fine-grained token': 'github' + '_pat_' + '11ABC'.repeat(6),
  'Authorization header': 'Authorization: ' + 'Bearer ' + 'tok.en-' + 'z'.repeat(20),
  'JSON Authorization header': '{"authorization": "' + 'Basic ' + 'dXNlcjpwYXNz"}',
  'OAuth 1.0 Authorization header': 'Authorization: ' + 'OAuth oauth_consumer_key="k", oauth_signature="s"',
  'bare Authorization token': 'authorization=' + 'Zx9'.repeat(8),
};

test('containsCredential detects each credential pattern', () => {
  for (const [name, secret] of Object.entries(CREDENTIALS)) {
    assert.equal(containsCredential(`noise before ${secret} noise after`), true, name);
  }
  for (const clean of ['line 1 ok', 'Authorization: required', 'Authorization: required for all endpoints', 'eyJ short', 'ghp_ short', 'see the private key docs']) {
    assert.equal(containsCredential(clean), false, clean);
  }
});

// '-' is a \b boundary, so a \b-anchored JWT pattern took over 100 s on this.
test('containsCredential stays linear on adversarial runs', () => {
  for (const unit of ['eyJ-', 'GOCSPX-', 'ya29.', 'github_pat_', 'abc1Q~']) {
    const started = Date.now();
    containsCredential(unit.repeat(50000));
    assert.ok(Date.now() - started < 5000, `${unit} took ${Date.now() - started} ms`);
  }
});

test('output carrying a credential is elided but never spilled', () => {
  for (const [name, secret] of Object.entries(CREDENTIALS)) {
    withSpillDir((dir) => {
      const lines = bigOutput(500).split('\n');
      lines[250] = secret;
      const text = lines.join('\n');
      const out = transform(text, FULL, 'Bash');
      assert.ok(out && out.length < text.length, `${name}: still must compress`);
      assert.doesNotMatch(out, /Full output:/, name);
      assert.ok(!exists(under(dir, 'chisle-spill')), `${name}: spilled to disk`);
    });
  }
});

test('clean output that only mentions credentials still spills', () => {
  withSpillDir((dir) => {
    const text = bigOutput(500) + '\nAuthorization: required\nprivate key rotation docs';
    const out = transform(text, FULL, 'Bash');
    const m = out.match(/Full output: (\S+)/);
    assert.ok(m, 'clean output lost its spill');
    assert.equal(readText(m[1]), text);
    assert.equal(listDir(under(dir, 'chisle-spill')).length, 1);
  });
});

test('a broken spill target still compresses', () => {
  withSpillDir((dir) => {
    // a file where the directory should be: mkdir fails, elision must not
    writeText(under(dir, 'chisle-spill'), 'not a directory');
    const text = bigOutput(500);
    const out = transform(text, FULL, 'Bash');
    assert.ok(out && out.length < text.length, 'a failed spill broke compression');
    assert.doesNotMatch(out, /Full output:/);
  });
});

// CHISLE_COMPRESS_TOOLS overrides the allowlist; it must not be able to switch
// off the Read/Edit/Write exclusion. That exclusion is the one thing standing
// between the compressor and a corrupted edit: Read output feeds Edit's
// old_string, so eliding it makes the model edit text it never saw. The Pi
// extension (NEVER_COMPRESS) and the OpenCode path (UNSAFE_TOOLS_OPENCODE)
// always enforced this; the Claude and Copilot paths used to let the env var
// through.
test('CHISLE_COMPRESS_TOOLS cannot re-enable Read/Edit/Write (Claude)', () => {
  const saved = process.env.CHISLE_COMPRESS_TOOLS;
  process.env.CHISLE_COMPRESS_TOOLS = 'Bash,Read,Edit,Write,NotebookEdit';
  try {
    for (const t of ['Read', 'Edit', 'Write', 'NotebookEdit']) {
      assert.equal(toolAllowed(t), false, `${t} must stay excluded`);
    }
    assert.equal(toolAllowed('Bash'), true, 'the override itself must still work');
  } finally {
    if (saved !== undefined) process.env.CHISLE_COMPRESS_TOOLS = saved;
    else delete process.env.CHISLE_COMPRESS_TOOLS;
  }
});

test('the exclusion is case-insensitive and survives an empty override', () => {
  const saved = process.env.CHISLE_COMPRESS_TOOLS;
  try {
    delete process.env.CHISLE_COMPRESS_TOOLS;
    for (const t of ['read', 'READ', 'ReAd', 'write', 'edit']) {
      assert.equal(toolAllowed(t), false, `${t} must stay excluded`);
    }
    // defaults still intact
    assert.equal(toolAllowed('Bash'), true);
    assert.equal(toolAllowed('mcp__server__tool'), true);
  } finally {
    if (saved !== undefined) process.env.CHISLE_COMPRESS_TOOLS = saved;
  }
});
