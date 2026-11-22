#!/usr/bin/env node
/* global __dirname, Buffer, process, require -- Node.js runtime */
// OpenCode tool-output compression tests — allowlist, compression decision,
// and the plugin's tool.execute.after hook mutating output in place.
// Run: node --test tests/test_opencode.js

const { beforeEach, test } = require('node:test');
const assert = require('node:assert/strict');
const {
  exists, listDir, makeDir, makeTempDir, readJson, readText, remove, under, writeText,
} = require('./helpers/sandbox');
const os = require('os');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const isolateWorkingDirectory = require('./fixtures/working-directory');
const { symlinkSkip } = require('./fixtures/symlinks');
const { spawn, spawnSync } = require('child_process');

// Give every test its own state root and let the fixture clean it up.
isolateWorkingDirectory();
beforeEach(t => {
  const previous = process.env.CHISLE_STATE_DIR;
  process.env.CHISLE_STATE_DIR = process.cwd();
  t.after(() => {
    if (previous === undefined) delete process.env.CHISLE_STATE_DIR;
    else process.env.CHISLE_STATE_DIR = previous;
  });
});

const opencodeCore = require('../hooks/chisle-compress-output');
const {
  opencodeToolAllowed, compressForOpencode, boundForOpencode, recordSavings,
  SAFE_TOOLS_OPENCODE, THRESHOLDS,
} = opencodeCore;
const { getOpencodeDir, getOpencodeStateDir } = require('../hooks/chisle-config');

function bigOutput(lines, prefix) {
  return Array.from({ length: lines }, (_, i) => `${prefix || 'line'} ${i} ${'x'.repeat(80)}`).join('\n');
}

function recoveryPath(output) {
  const match = output.match(/Full output: (.+?) \(grep it, do not re-run\)/);
  assert.ok(match);
  return match[1];
}

function hasUnpairedSurrogate(value) {
  return /[\uD800-\uDFFF]/u.test(value);
}

function sessionStateFile(stateDir, sessionId) {
  const name = crypto.createHash('sha256').update(sessionId).digest('hex') + '.json';
  return under(stateDir, 'chisle-dedup', name);
}

// ── config root ──────────────────────────────────────────────────────────────

test('OpenCode root follows host precedence and keeps an injectable home fallback', () => {
  const home = under(os.tmpdir(), 'chisle-home');
  assert.equal(getOpencodeDir({ env: {}, home }), under(home, '.config', 'opencode'));
  assert.equal(getOpencodeDir({ env: { XDG_CONFIG_HOME: '/xdg' }, home }), under('/xdg', 'opencode'));
  assert.equal(getOpencodeDir({ env: { XDG_CONFIG_HOME: '/xdg', OPENCODE_CONFIG_DIR: '/custom' }, home }), '/custom');
});

test('OpenCode state uses its config root unless CHISLE_STATE_DIR overrides it', () => {
  const home = under(os.tmpdir(), 'chisle-home');
  assert.equal(getOpencodeStateDir({ env: { OPENCODE_CONFIG_DIR: '/custom' }, home }), '/custom');
  assert.equal(getOpencodeStateDir({ env: { OPENCODE_CONFIG_DIR: '/custom', CHISLE_STATE_DIR: '/state' }, home }), '/state');
});

// ── allowlist ────────────────────────────────────────────────────────────────

test('opencodeToolAllowed allows only explicit read-heavy defaults', () => {
  const previous = process.env.CHISLE_COMPRESS_TOOLS;
  delete process.env.CHISLE_COMPRESS_TOOLS;
  try {
    assert.deepEqual(SAFE_TOOLS_OPENCODE, ['bash', 'shell', 'grep', 'glob', 'webfetch', 'websearch', 'list']);
    for (const t of SAFE_TOOLS_OPENCODE) assert.equal(opencodeToolAllowed(t), true, t);
  } finally {
    if (previous === undefined) delete process.env.CHISLE_COMPRESS_TOOLS;
    else process.env.CHISLE_COMPRESS_TOOLS = previous;
  }
});

test('opencodeToolAllowed excludes source-bearing and mutating tools', () => {
  for (const t of [
    'read', 'edit', 'write', 'patch', 'apply_patch', 'multiedit',
    'notebookedit', 'notebookread', 'notebook_write', 'notebook_edit',
    'create', 'str_replace', 'insert', 'todowrite', 'todoread',
  ]) assert.equal(opencodeToolAllowed(t), false, t);
});

test('opencodeToolAllowed does not infer safety from underscores', () => {
  assert.equal(opencodeToolAllowed('github-tools_search_code'), false);
  assert.equal(opencodeToolAllowed('unknown_mcp_tool'), false);
});

test('opencodeToolAllowed rejects junk', () => {
  assert.equal(opencodeToolAllowed(''), false);
  assert.equal(opencodeToolAllowed(null), false);
  assert.equal(opencodeToolAllowed(undefined), false);
});

test('CHISLE_COMPRESS_TOOLS override is an exact allowlist', () => {
  process.env.CHISLE_COMPRESS_TOOLS = 'bash,grep';
  try {
    assert.equal(opencodeToolAllowed('bash'), true);
    assert.equal(opencodeToolAllowed('glob'), false);       // not in override
    assert.equal(opencodeToolAllowed('github-tools_x'), false); // override wins over MCP heuristic
  } finally {
    delete process.env.CHISLE_COMPRESS_TOOLS;
  }
});

test('CHISLE_COMPRESS_TOOLS allows explicit MCP tools but not protected tools', () => {
  process.env.CHISLE_COMPRESS_TOOLS = 'custom_report,read,edit,write,patch,multiedit,notebookread';
  try {
    assert.equal(opencodeToolAllowed('custom_report'), true);
    for (const t of ['read', 'edit', 'write', 'patch', 'multiedit', 'notebookread']) {
      assert.equal(opencodeToolAllowed(t), false, t);
    }
  } finally {
    delete process.env.CHISLE_COMPRESS_TOOLS;
  }
});

// ── compression decision ─────────────────────────────────────────────────────

test('compressForOpencode elides a big bash output', () => {
  const text = bigOutput(500);
  const out = compressForOpencode('bash', text, { mode: 'on' });
  assert.ok(out && out.length < text.length);
  assert.match(out, /chisle: elided/);
});

test('compressForOpencode leaves small output untouched', () => {
  const out = compressForOpencode('bash', 'short output', { mode: 'on' });
  assert.equal(out, null);
});

test('compressForOpencode never touches read output even when huge', () => {
  const out = compressForOpencode('read', bigOutput(500), { mode: 'on' });
  assert.equal(out, null);
});

test('compressForOpencode honors mode off and the kill switch', () => {
  const text = bigOutput(500);
  assert.equal(compressForOpencode('bash', text, { mode: 'off' }), null);
  process.env.CHISLE_COMPRESS = '0';
  try {
    assert.equal(compressForOpencode('bash', text, { mode: 'on' }), null);
  } finally {
    delete process.env.CHISLE_COMPRESS;
  }
});

test('compressForOpencode salvages error lines from the elided middle', () => {
  const lines = [];
  for (let i = 0; i < 60; i++) lines.push(`head ${i}`);
  lines.push('ERROR: the one line that mattered');
  for (let i = 0; i < 400; i++) lines.push(`noise ${i} ${'y'.repeat(60)}`);
  for (let i = 0; i < 40; i++) lines.push(`tail ${i}`);
  const out = compressForOpencode('bash', lines.join('\n'), { mode: 'on' });
  assert.match(out, /ERROR: the one line that mattered/);
});

test('salvage matches severities inside underscore codes and ERR!, not inside words (#38)', () => {
  const keep = ['FATAL_MEMORY_PROBE: id-1', 'APP_ERROR_CODE: id-2', 'WARNING_LIMIT: id-3', 'npm ERR! code E404', 'ERR!attached'];
  const drop = ['fatalistic outlook', 'terror level', 'warningly said'];
  const lines = Array.from({ length: 450 }, (_, i) => `build ${i} ${'x'.repeat(70)}`);
  [...keep, ...drop].forEach((l, i) => lines.splice(200 + i * 5, 0, l));
  const out = compressForOpencode('bash', lines.join('\n'), { mode: 'on' });
  for (const l of keep) assert.ok(out.includes(l), 'dropped: ' + l);
  for (const l of drop) assert.ok(!out.includes(l), 'salvaged: ' + l);
});

test('maxChars is a final bound for long lines with late errors', () => {
  const stateDir = makeTempDir('chisle-oc-bound-');
  process.env.CHISLE_COMPRESS_MAX_CHARS = '4000';
  try {
    const text = [
      `BEGIN ${'a'.repeat(5000)}`,
      `${'b'.repeat(5000)} FATAL LATE_ERROR ${'c'.repeat(5000)}`,
      `${'d'.repeat(5000)} END`,
    ].join('\n');
    const out = compressForOpencode('bash', text, { mode: 'on', stateDir });
    assert.ok(out.length <= 4000);
    assert.match(out, /BEGIN/);
    assert.match(out, /END/);
    assert.match(out, /FATAL LATE_ERROR/);
    assert.equal(readText(recoveryPath(out)), text);
  } finally {
    delete process.env.CHISLE_COMPRESS_MAX_CHARS;
    remove(stateDir, { recursive: true, force: true });
  }
});

test('maxChars remains exact at its smallest valid value', () => {
  const previous = process.env.CHISLE_COMPRESS_MAX_CHARS;
  process.env.CHISLE_COMPRESS_MAX_CHARS = '1';
  try {
    assert.equal(compressForOpencode('bash', 'x'.repeat(5000), { mode: 'on' }).length, 1);
  } finally {
    if (previous === undefined) delete process.env.CHISLE_COMPRESS_MAX_CHARS;
    else process.env.CHISLE_COMPRESS_MAX_CHARS = previous;
  }
});

test('maxChars bounds short output and duplicate markers', () => {
  const previous = process.env.CHISLE_COMPRESS_MAX_CHARS;
  const stateDir = makeTempDir('chisle-oc-small-bound-');
  try {
    process.env.CHISLE_COMPRESS_MAX_CHARS = '1';
    assert.equal(compressForOpencode('bash', 'x'.repeat(100), { mode: 'on', stateDir }).length, 1);
    process.env.CHISLE_COMPRESS_MAX_CHARS = '1000';
    const text = Array(5).fill('x'.repeat(300)).join('\n') + '\n' + 'y'.repeat(3000);
    const opts = { mode: 'on', stateDir, sessionId: 'bounded-dedup' };
    compressForOpencode('bash', text, { ...opts, callId: 'first' });
    const duplicate = compressForOpencode('bash', text, { ...opts, callId: 'second' });
    assert.ok(duplicate.length <= 1000);
    assert.match(duplicate, /byte-identical/);
  } finally {
    if (previous === undefined) delete process.env.CHISLE_COMPRESS_MAX_CHARS;
    else process.env.CHISLE_COMPRESS_MAX_CHARS = previous;
    remove(stateDir, { recursive: true, force: true });
  }
});

test('small bounds retain late diagnostics and head/tail context', () => {
  const previous = process.env.CHISLE_COMPRESS_MAX_CHARS;
  process.env.CHISLE_COMPRESS_MAX_CHARS = '1000';
  try {
    const text = `BEGIN ${'a'.repeat(5000)} FATAL LATE_ERROR ${'b'.repeat(5000)} END`;
    const out = compressForOpencode('bash', text, { mode: 'on' });
    assert.ok(out.length <= 1000);
    assert.match(out, /BEGIN/);
    assert.match(out, /FATAL LATE_ERROR/);
    assert.match(out, /END/);
    assert.equal(readText(recoveryPath(out)), text);
  } finally {
    if (previous === undefined) delete process.env.CHISLE_COMPRESS_MAX_CHARS;
    else process.env.CHISLE_COMPRESS_MAX_CHARS = previous;
  }
});

test('spill preserves the byte-identical pre-scrub output', () => {
  const stateDir = makeTempDir('chisle-oc-spill-');
  try {
    const text = `\u001b[31mBEGIN\u001b[0m   \n${bigOutput(200)}\n\n\nEND`;
    const out = compressForOpencode('bash', text, { mode: 'on', stateDir });
    const recovery = recoveryPath(out);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Reads raw bytes of a file this test created under os.tmpdir().
    assert.deepEqual(fs.readFileSync(recovery), Buffer.from(text)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Inspects a file this test created under os.tmpdir().
    if (process.platform !== 'win32') assert.equal(fs.statSync(recovery).mode & 0o777, 0o600); // NTFS has no POSIX mode bits // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    assert.deepEqual(listDir(path.dirname(recovery)).filter(name => name.endsWith('.tmp')), []);
  } finally {
    remove(stateDir, { recursive: true, force: true });
  }
});

test('spill rejects symlinked directories and final files', { skip: process.platform === 'win32' }, () => {
  const stateDir = makeTempDir('chisle-oc-spill-link-');
  const outside = makeTempDir('chisle-oc-spill-outside-');
  try {
    const text = bigOutput(200, 'linked');
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(outside, under(stateDir, 'chisle-spill')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    const dirOutput = compressForOpencode('bash', text, { mode: 'on', stateDir });
    assert.doesNotMatch(dirOutput, /Full output:/);
    assert.deepEqual(listDir(outside), []);

    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Removes a link this test created under os.tmpdir().
    fs.unlinkSync(under(stateDir, 'chisle-spill')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    makeDir(under(stateDir, 'chisle-spill'));
    const hash = crypto.createHash('sha256').update(text).digest('hex').slice(0, 12);
    const victim = under(outside, 'victim');
    writeText(victim, 'unchanged');
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(victim, under(stateDir, 'chisle-spill', `bash-${hash}.txt`)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    const fileOutput = compressForOpencode('bash', text, { mode: 'on', stateDir });
    assert.doesNotMatch(fileOutput, /Full output:/);
    assert.equal(readText(victim), 'unchanged');
  } finally {
    remove(stateDir, { recursive: true, force: true });
    remove(outside, { recursive: true, force: true });
  }
});

test('a repeated spill reuses the existing file even when UTF-8 cannot round-trip it', () => {
  const stateDir = makeTempDir('chisle-oc-spill-reuse-');
  try {
    const text = bigOutput(200, 'lone \uD800');
    const first = compressForOpencode('bash', text, { mode: 'on', stateDir });
    const second = compressForOpencode('bash', text, { mode: 'on', stateDir });
    assert.match(first, /Full output:/);
    assert.match(second, /Full output:/);
  } finally {
    remove(stateDir, { recursive: true, force: true });
  }
});

test('spill and stats reject symlinked randomized temporary targets', { skip: process.platform === 'win32' }, () => {
  const root = makeTempDir('chisle-oc-tmp-link-');
  const core = under(__dirname, '..', 'hooks', 'chisle-compress-output.js');
  try {
    const probe = `
      const fs=require('fs'),path=require('path'),crypto=require('crypto');
      crypto.randomBytes=()=>Buffer.from('010203040506','hex');
      const c=require(${JSON.stringify(core)}),root=process.argv[1],victim=path.join(root,'victim');
      fs.writeFileSync(victim,'unchanged');
      const text=Array.from({length:200},(_,i)=>'temporary '+i+' '+'x'.repeat(80)).join('\\n');
      const spillDir=path.join(root,'spill','chisle-spill');fs.mkdirSync(spillDir,{recursive:true});
      const hash=crypto.createHash('sha256').update(text).digest('hex').slice(0,12);
      fs.symlinkSync(victim,path.join(spillDir,'bash-'+hash+'.txt.'+process.pid+'.010203040506.tmp'));
      const out=c.compressForOpencode('bash',text,{mode:'on',stateDir:path.join(root,'spill')});
      const statsDir=path.join(root,'stats');fs.mkdirSync(statsDir);
      fs.symlinkSync(victim,path.join(statsDir,'.chisle-compress-stats.json.'+process.pid+'.010203040506.tmp'));
      c.recordSavings(10,statsDir);
      process.stdout.write(JSON.stringify({recovery:/Full output:/.test(out),ledger:fs.existsSync(path.join(statsDir,'.chisle-compress-stats.json')),victim:fs.readFileSync(victim,'utf8')}));
    `;
    const result = spawnSync(process.execPath, ['-e', probe, root], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), { recovery: false, ledger: false, victim: 'unchanged' });
  } finally {
    remove(root, { recursive: true, force: true });
  }
});

// Symlinked state ROOT is a supported user layout (dotfiles via GNU stow), so
// recordSavings must still write there. The files Chisle writes into the root
// are kept symlink-free.
test('stats writes are private and reject a symlinked stats file', { skip: process.platform === 'win32' }, () => {
  const root = makeTempDir('chisle-oc-stats-safe-');
  const outside = makeTempDir('chisle-oc-stats-outside-');
  try {
    const normal = under(root, 'normal');
    recordSavings(12, normal);
    const ledger = under(normal, '.chisle-compress-stats.json');
    assert.deepEqual(readJson(ledger), { savedChars: 12, events: 1 });
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Inspects a file this test created under os.tmpdir().
    assert.equal(fs.statSync(ledger).mode & 0o777, 0o600); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

    const linkedFileDir = under(root, 'linked-file');
    makeDir(linkedFileDir);
    const victim = under(outside, 'victim');
    writeText(victim, 'unchanged');
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(victim, under(linkedFileDir, '.chisle-compress-stats.json')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    recordSavings(30, linkedFileDir);
    assert.equal(readText(victim), 'unchanged');
  } finally {
    remove(root, { recursive: true, force: true });
    remove(outside, { recursive: true, force: true });
  }
});

// A user-owned state ROOT that is itself a symlink (dotfiles via GNU stow)
// must still record — only the files/subdirs Chisle creates beneath it are
// kept symlink-free. Regression guard for PR #33 review. Symlink support is
// probed up front so an environment that can't create links skips cleanly.
test('recordSavings, spill, and dedup accept a symlinked state root', (t) => {
  const probe = makeTempDir('chisle-oc-symprobe-');
  let linkOk = true;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(under(probe, 'no-target'), under(probe, 'link'), 'dir'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
  } catch { linkOk = false; }
  finally { remove(probe, { recursive: true, force: true }); }
  if (!linkOk) { t.skip('host cannot create symlinks'); return; }

  const outside = makeTempDir('chisle-oc-symroot-');
  const victimDir = makeTempDir('chisle-oc-symroot-victim-');
  const root = under(path.dirname(outside), path.basename(outside) + '-link');
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(outside, root, 'dir'); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

    // Stats ledger records through the symlinked root.
    recordSavings(100, root);
    const ledger = under(outside, '.chisle-compress-stats.json');
    assert.deepEqual(readJson(ledger), { savedChars: 100, events: 1 });

    // Spill and per-session dedup state under the symlinked root still write.
    const text = bigOutput(200, 'rooted');
    const output = compressForOpencode('bash', text, { mode: 'on', stateDir: root, sessionId: 's1', callId: 'c1' });
    const recovery = recoveryPath(output);
    // The hook returns the path as given (the symlink); the file must exist
    // inside the REAL directory the root points at.
    assert.equal(path.dirname(recovery), under(root, 'chisle-spill'));
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Resolves a directory this test created under os.tmpdir().
    assert.equal(fs.realpathSync(path.dirname(recovery)), under(outside, 'chisle-spill')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Reads raw bytes of a file this test created under os.tmpdir().
    assert.deepEqual(fs.readFileSync(recovery), Buffer.from(text)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    assert.equal(exists(under(outside, 'chisle-dedup')), true);

    // ...but a symlink planted by Chisle INSIDE the root is still rejected.
    const victim = under(victimDir, 'victim');
    writeText(victim, 'unchanged');
    remove(ledger); // symlinkSync cannot replace an existing file
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(victim, ledger); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    recordSavings(100, root);
    assert.equal(readText(victim), 'unchanged');
  } finally {
    remove(root, { recursive: true, force: true });
    remove(outside, { recursive: true, force: true });
    remove(victimDir, { recursive: true, force: true });
  }
});

test('OpenCode truncation is Unicode-safe', () => {
  const stateDir = makeTempDir('chisle-oc-unicode-');
  try {
    const text = `${'😀'.repeat(3000)}\n${'😀'.repeat(400)} FATAL EMOJI_ERROR ${'😀'.repeat(400)}\n${'😀'.repeat(3000)}`;
    const out = compressForOpencode('bash', text, { mode: 'on', stateDir });
    assert.equal(hasUnpairedSurrogate(out), false);
    assert.match(out, /FATAL EMOJI_ERROR/);
    assert.equal(hasUnpairedSurrogate(boundForOpencode(text, '/tmp/emoji.txt', 1001)), false);
  } finally {
    remove(stateDir, { recursive: true, force: true });
  }
});

// ── plugin hook wiring ───────────────────────────────────────────────────────

async function loadV2Plugin() {
  const hooks = [];
  const setup = require('../.opencode/plugins-v2/chisle/setup.cjs');
  await setup({ tool: { hook: async (name, callback) => hooks.push([name, callback]) } });
  assert.equal(hooks.length, 1);
  assert.equal(hooks[0][0], 'execute.after');
  return hooks[0][1];
}

test('OpenCode v2 plugin uses Plugin.define and registers execute.after', async () => {
  const entry = readText(under(__dirname, '..', '.opencode', 'plugins-v2', 'chisle', 'index.mjs'));
  assert.match(entry, /import \{ Plugin \} from '@opencode\/plugin'/);
  assert.match(entry, /Plugin\.define\(\{ id: 'chisle', setup \}\)/);
  assert.equal(typeof await loadV2Plugin(), 'function');
});

test('OpenCode v2 shell compression preserves spill, metadata, safety, stats, and idempotency', async () => {
  const stateDir = makeTempDir('chisle-oc-v2-');
  const previous = process.env.CHISLE_STATE_DIR;
  const previousTools = process.env.CHISLE_COMPRESS_TOOLS;
  process.env.CHISLE_STATE_DIR = stateDir;
  process.env.CHISLE_COMPRESS_TOOLS = 'shell';
  try {
    const after = await loadV2Plugin();
    const original = bigOutput(500, 'shell');
    const event = {
      tool: 'shell', sessionID: 'v2', id: 'one', status: 'completed', input: {},
      result: { output: { output: original, exit: 0 }, content: [{ type: 'text', text: original }], metadata: { exit: 0 } },
    };
    await after(event);
    const compressed = event.result.content[0].text;
    assert.ok(compressed.length < original.length);
    assert.match(compressed, /chisle: elided/);
    assert.deepEqual(event.result.output, { output: original, exit: 0 });
    assert.deepEqual(event.result.metadata, { exit: 0 });
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Reads raw bytes of a file this test created under os.tmpdir().
    assert.deepEqual(fs.readFileSync(recoveryPath(compressed)), Buffer.from(original)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    assert.deepEqual(readJson(under(stateDir, '.chisle-compress-stats.json')), {
      savedChars: original.length - compressed.length, events: 1,
    });

    await after(event);
    assert.equal(event.result.content[0].text, compressed);

    for (const tool of ['read', 'edit', 'write']) {
      const protectedEvent = {
        tool, sessionID: 'v2', id: tool, status: 'completed', input: {},
        result: { content: original, metadata: {} },
      };
      await after(protectedEvent);
      assert.equal(protectedEvent.result.content, original);
    }
  } finally {
    if (previous === undefined) delete process.env.CHISLE_STATE_DIR;
    else process.env.CHISLE_STATE_DIR = previous;
    if (previousTools === undefined) delete process.env.CHISLE_COMPRESS_TOOLS;
    else process.env.CHISLE_COMPRESS_TOOLS = previousTools;
    remove(stateDir, { recursive: true, force: true });
  }
});

test('OpenCode v2 plugin fails open when compression throws', async () => {
  const originalCompress = opencodeCore.compressForOpencode;
  opencodeCore.compressForOpencode = () => { throw new Error('compressor failed'); };
  try {
    const after = await loadV2Plugin();
    const original = bigOutput(500, 'error');
    const event = {
      tool: 'shell', sessionID: 'v2', id: 'error', status: 'completed', input: {},
      result: { content: original, metadata: {} },
    };
    await assert.doesNotReject(after(event));
    assert.equal(event.result.content, original);
  } finally {
    opencodeCore.compressForOpencode = originalCompress;
  }
});

test('plugin tool.execute.after compresses output.output in place', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const after = hooks['tool.execute.after'];
  assert.equal(typeof after, 'function');

  const big = bigOutput(500);
  const output = { title: 't', output: big, metadata: {} };
  await after({ tool: 'bash', sessionID: 's', callID: 'c', args: {} }, output);
  assert.ok(output.output.length < big.length);
  assert.match(output.output, /chisle: elided/);

  // read is never compressed
  const readOut = { title: 't', output: big, metadata: {} };
  await after({ tool: 'read', sessionID: 's', callID: 'c2', args: {} }, readOut);
  assert.equal(readOut.output, big);
});

test('failed Bash results are compressed once without changing exit metadata', async () => {
  const stateDir = makeTempDir('chisle-oc-failed-');
  const previous = process.env.CHISLE_STATE_DIR;
  process.env.CHISLE_STATE_DIR = stateDir;
  try {
    const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
    const hooks = await plugin({});
    const original = `${bigOutput(220, 'failed')}\nFATAL_FAILED_SMOKE exit=23`;
    const output = { output: original, metadata: { exit: 23, truncated: false } };
    await hooks['tool.execute.after']({ tool: 'bash', sessionID: 'failed', callID: 'failed-1' }, output);

    assert.ok(output.output.length <= THRESHOLDS.maxChars);
    assert.match(output.output, /FATAL_FAILED_SMOKE exit=23/);
    assert.deepEqual(output.metadata, { exit: 23, truncated: false });
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Reads raw bytes of a file this test created under os.tmpdir().
    assert.deepEqual(fs.readFileSync(recoveryPath(output.output)), Buffer.from(original)); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename

    const statsPath = under(stateDir, '.chisle-compress-stats.json');
    const stats = readJson(statsPath);
    assert.deepEqual(stats, { savedChars: original.length - output.output.length, events: 1 });

    const messages = [{ parts: [{ type: 'tool', tool: 'bash', state: { status: 'completed', output: output.output } }] }];
    await hooks['experimental.chat.messages.transform']({}, { messages });
    assert.deepEqual(readJson(statsPath), stats);
  } finally {
    if (previous === undefined) delete process.env.CHISLE_STATE_DIR;
    else process.env.CHISLE_STATE_DIR = previous;
    remove(stateDir, { recursive: true, force: true });
  }
});

test('plugin hook never throws on odd output shapes', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const after = hooks['tool.execute.after'];
  await after({ tool: 'bash' }, { output: null });
  await after({ tool: 'bash' }, {});
  await after({}, { output: 'x' });
  // MCP tools can deliver content[] arrays instead of a string — skip, never throw.
  await after({ tool: 'github-tools_x' }, { output: [{ type: 'text', text: 'hi' }] });
  // no throw = pass
});

// ── experimental.chat.messages.transform (request-time / retroactive) ────────

test('messages.transform compresses a big completed tool part in place', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const transform = hooks['experimental.chat.messages.transform'];
  assert.equal(typeof transform, 'function');

  const big = bigOutput(500);
  const messages = [{
    info: { role: 'assistant' },
    parts: [
      { type: 'text', text: 'thinking' },
      { type: 'tool', tool: 'bash', callID: 'c1', state: { status: 'completed', output: big, input: {} } },
    ],
  }];
  await transform({}, { messages });
  const toolPart = messages[0].parts[1];
  assert.ok(toolPart.state.output.length < big.length);
  assert.match(toolPart.state.output, /chisle: elided/);
});

test('messages.transform preserves errors and leaves protected/small parts alone', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const transform = hooks['experimental.chat.messages.transform'];

  const lines = [];
  for (let i = 0; i < 60; i++) lines.push(`head ${i}`);
  lines.push('ERROR: keep me');
  for (let i = 0; i < 400; i++) lines.push(`noise ${i} ${'y'.repeat(60)}`);
  for (let i = 0; i < 40; i++) lines.push(`tail ${i}`);
  const bashBig = lines.join('\n');
  const readBig = bigOutput(500);
  const small = 'short';
  const messages = [{
    info: { role: 'assistant' },
    parts: [
      { type: 'tool', tool: 'bash', callID: 'a', state: { status: 'completed', output: bashBig, input: {} } },
      { type: 'tool', tool: 'read', callID: 'b', state: { status: 'completed', output: readBig, input: {} } },
      { type: 'tool', tool: 'bash', callID: 'c', state: { status: 'completed', output: small, input: {} } },
      { type: 'tool', tool: 'bash', callID: 'd', state: { status: 'running', input: {} } },
    ],
  }];
  await transform({}, { messages });
  const [p0, p1, p2, p3] = messages[0].parts;
  assert.match(p0.state.output, /ERROR: keep me/);      // error salvaged
  assert.ok(p0.state.output.length < bashBig.length);   // bash compressed
  assert.equal(p1.state.output, readBig);               // read untouched
  assert.equal(p2.state.output, small);                 // small untouched
  assert.equal(p3.state.output, undefined);             // non-completed untouched
});

test('messages.transform honors mode off / kill switch and never throws', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const transform = hooks['experimental.chat.messages.transform'];

  const big = bigOutput(500);
  process.env.CHISLE_COMPRESS = '0';
  try {
    const messages = [{ info: {}, parts: [{ type: 'tool', tool: 'bash', state: { status: 'completed', output: big, input: {} } }] }];
    await transform({}, { messages });
    assert.equal(messages[0].parts[0].state.output, big); // kill switch: untouched
  } finally {
    delete process.env.CHISLE_COMPRESS;
  }
  // odd shapes: no throw
  await transform({}, {});
  await transform({}, { messages: null });
  await transform({}, { messages: [null, { parts: null }, { parts: [null, { type: 'text' }] }] });
});

test('messages.transform is idempotent: already-compressed parts are skipped', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const transform = hooks['experimental.chat.messages.transform'];

  const big = bigOutput(500);
  const messages = [{ info: {}, parts: [{ type: 'tool', tool: 'bash', state: { status: 'completed', output: big, input: {} } }] }];
  await transform({}, { messages });
  const once = messages[0].parts[0].state.output;
  assert.match(once, /chisle: elided/);
  await transform({}, { messages });            // second pass
  assert.equal(messages[0].parts[0].state.output, once); // unchanged, no nested marker
});

test('tool.execute.after preserves bounded Chisle output but bounds source markers', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const after = hooks['tool.execute.after'];
  const already = 'kept\n... [chisle: elided 100 lines] ...';
  const first = { title: 't', output: already, metadata: {} };
  await after({ tool: 'bash', callID: 'c1' }, first);
  assert.equal(first.output, already);

  const source = bigOutput(500) + '\n... [chisle: source marker] ...';
  const second = { title: 't', output: source, metadata: {} };
  await after({ tool: 'bash', sessionID: 'marker-session', callID: 'c2' }, second);
  assert.ok(second.output.length <= THRESHOLDS.maxChars);
  assert.notEqual(second.output, source);
});

test('OpenCode-native tool_output truncation remains recoverable before the hook', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const nativePath = '/tmp/opencode/tool_original';
  const native = `${bigOutput(300)}\n\n...250000 bytes truncated...\n\nThe tool call succeeded but the output was truncated. Full output saved to: ${nativePath}\nUse Grep to search the full content or Read with offset/limit to view specific sections.`;
  const output = { output: native };
  await hooks['tool.execute.after']({ tool: 'bash', sessionID: 'ordered', callID: 'ordered-1' }, output);
  assert.ok(output.output.length <= THRESHOLDS.maxChars);
  assert.ok(output.output.includes(nativePath));
});

test('OpenCode dedup is isolated across interleaved sessions and calls', () => {
  const stateDir = makeTempDir('chisle-oc-dedup-');
  try {
    const text = `same\n${'x'.repeat(3000)}`;
    assert.equal(compressForOpencode('bash', text, { mode: 'on', stateDir, sessionId: 'a', callId: 'a1' }), null);
    assert.equal(compressForOpencode('bash', text, { mode: 'on', stateDir, sessionId: 'b', callId: 'b1' }), null);
    assert.equal(compressForOpencode('bash', text, { mode: 'on', stateDir, sessionId: 'a', callId: 'a1' }), null);
    assert.match(compressForOpencode('bash', text, { mode: 'on', stateDir, sessionId: 'a', callId: 'a2' }), /byte-identical/);
    assert.equal(exists(sessionStateFile(stateDir, 'a')), true);
    assert.equal(exists(sessionStateFile(stateDir, 'b')), true);
  } finally {
    remove(stateDir, { recursive: true, force: true });
  }
});

test('OpenCode dedup state is bounded and does not follow symlinks', () => {
  const stateDir = makeTempDir('chisle-oc-dedup-safe-');
  try {
    for (let i = 0; i < 25; i++) {
      compressForOpencode('bash', `session ${i}\n${'x'.repeat(3000)}`, {
        mode: 'on', stateDir, sessionId: `s${i}`, callId: `c${i}`,
      });
    }
    const dir = under(stateDir, 'chisle-dedup');
    assert.equal(listDir(dir).filter((name) => name.endsWith('.json')).length, 20);
    if (symlinkSkip) return; // the bound above holds everywhere; the symlink half needs symlinks
    const target = under(stateDir, 'target');
    writeText(target, 'unchanged');
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- Creates a test fixture link under os.tmpdir() or the test working directory.
    fs.symlinkSync(target, sessionStateFile(stateDir, 'linked')); // nosemgrep: javascript_pathtraversal_rule-non-literal-fs-filename
    compressForOpencode('bash', `linked\n${'x'.repeat(3000)}`, {
      mode: 'on', stateDir, sessionId: 'linked', callId: 'linked-1',
    });
    assert.equal(readText(target), 'unchanged');
  } finally {
    remove(stateDir, { recursive: true, force: true });
  }
});

test('concurrent OpenCode sessions retain independent dedup state', async () => {
  const stateDir = makeTempDir('chisle-oc-concurrent-');
  const core = under(__dirname, '..', 'hooks', 'chisle-compress-output.js');
  try {
    const sessions = Array.from({ length: 8 }, (_, i) => `concurrent-${i}`);
    await Promise.all(sessions.map((session) => new Promise((resolve, reject) => {
      const script = `const c=require(${JSON.stringify(core)});c.compressForOpencode('bash','${'x'.repeat(2100)}',{mode:'on',stateDir:process.argv[1],sessionId:process.argv[2],callId:'first'})`;
      const child = spawn(process.execPath, ['-e', script, stateDir, session], { stdio: 'ignore' });
      child.on('error', reject);
      child.on('exit', (code) => code === 0 ? resolve() : reject(new Error(`child exited ${code}`)));
    })));
    for (const session of sessions) {
      const duplicate = compressForOpencode('bash', 'x'.repeat(2100), {
        mode: 'on', stateDir, sessionId: session, callId: 'second',
      });
      assert.match(duplicate, /byte-identical/);
    }
  } finally {
    remove(stateDir, { recursive: true, force: true });
  }
});

test('only the persisted after-hook replacement records savings', async () => {
  const stateDir = makeTempDir('chisle-oc-stats-');
  const previous = process.env.CHISLE_STATE_DIR;
  process.env.CHISLE_STATE_DIR = stateDir;
  try {
    const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
    const hooks = await plugin({});
    const big = bigOutput(500, 'stats');
    const output = { output: big };
    await hooks['tool.execute.after']({ tool: 'bash', sessionID: 'stats', callID: 'one' }, output);
    const statsPath = under(stateDir, '.chisle-compress-stats.json');
    const first = readJson(statsPath);
    const messages = [{ parts: [{ type: 'tool', tool: 'bash', state: { status: 'completed', output: big } }] }];
    await hooks['experimental.chat.messages.transform']({}, { messages });
    assert.deepEqual(readJson(statsPath), first);
  } finally {
    if (previous === undefined) delete process.env.CHISLE_STATE_DIR;
    else process.env.CHISLE_STATE_DIR = previous;
    remove(stateDir, { recursive: true, force: true });
  }
});

// ── installed layout: CJS core under a type:module config dir ───────────────
// OpenCode's config dir declares "type": "module". Node applies that to every
// .js file beneath it, so without an explicit pin the CommonJS compressor core
// is parsed as ESM and throws ReferenceError on its first `require` — which
// loadFrom rethrows, taking the whole plugin down. Bun tolerates it; Node does
// not. This asserts the installed layout survives the strict runtime.
test('plugin runtime keeps spill, dedup, and stats under OPENCODE_CONFIG_DIR', () => {
  const root = makeTempDir('chisle-oc-runtime-root-');
  try {
    const pluginPath = under(__dirname, '..', '.opencode', 'plugins', 'chisle.mjs');
    const probe = `
      const big = Array.from({length:220},(_,i)=>'line '+i+' '+'x'.repeat(80)).join('\\n');
      import(${JSON.stringify('file://' + pluginPath)}).then(async (m) => {
        const hooks = await m.default({});
        const out = { output: big, metadata: { exit: 23 } };
        await hooks['tool.execute.after']({ tool: 'bash', sessionID: 'root', callID: 'one' }, out);
        process.stdout.write(out.output.length < big.length ? 'COMPRESSED' : 'NOOP');
      });
    `;
    const env = { ...process.env, OPENCODE_CONFIG_DIR: root, XDG_CONFIG_HOME: '', CHISLE_STATE_DIR: '' };
    const result = spawnSync(process.execPath, ['--input-type=module', '-e', probe], { encoding: 'utf8', env });
    assert.equal(result.stdout, 'COMPRESSED', result.stderr);
    assert.ok(listDir(under(root, 'chisle-spill')).some(name => name.endsWith('.txt')));
    assert.ok(listDir(under(root, 'chisle-dedup')).some(name => name.endsWith('.json')));
    assert.ok(exists(under(root, '.chisle-compress-stats.json')));
  } finally {
    remove(root, { recursive: true, force: true });
  }
});

test('installed plugin still compresses when the config dir is type:module', async () => {
  const home = makeTempDir('chisle-oc-mod-');
  try {
    const { spawnSync } = require('child_process');
    const cli = under(__dirname, '..', 'bin', 'install.js');
    const r = spawnSync(process.execPath, [cli, '--only', 'opencode'], {
      encoding: 'utf8',
      env: Object.assign({}, process.env, {
        HOME: home, USERPROFILE: home, CHISLE_HOME: home,
        XDG_CONFIG_HOME: '', OPENCODE_CONFIG_DIR: '',
      }),
    });
    assert.equal(r.status, 0, r.stderr);

    const oc = under(home, '.config', 'opencode');
    writeText(under(oc, 'package.json'), '{ "type": "module" }\n');

    const probe = `
      const big = Array.from({length:500},(_,i)=>'line '+i+' '+'x'.repeat(80)).join('\\n');
      import(${JSON.stringify('file://' + under(oc, 'plugins', 'chisle.js'))})
        .then(async (m) => {
          const hooks = await m.default({});
          const out = { output: big };
          await hooks['tool.execute.after']({ tool: 'bash' }, out);
          process.stdout.write(out.output.length < big.length ? 'COMPRESSED' : 'NOOP');
        });
    `;
    const p = spawnSync(process.execPath, ['--input-type=module', '-e', probe], { encoding: 'utf8' });
    assert.equal(p.stdout, 'COMPRESSED', 'core failed to load under type:module: ' + p.stderr);
  } finally {
    remove(home, { recursive: true, force: true });
  }
});

test('plugin hooks leave output alone when reading it throws', async () => {
  const { default: plugin } = await import('../.opencode/plugins/chisle.mjs');
  const hooks = await plugin({});
  const broken = { get output() { throw new Error('unreadable output'); } };
  await hooks['tool.execute.after']({ tool: 'bash', sessionID: 's', callID: 'c', args: {} }, broken);
  const messages = [{ get parts() { throw new Error('unreadable parts'); } }];
  await hooks['experimental.chat.messages.transform']({}, { messages });
});
