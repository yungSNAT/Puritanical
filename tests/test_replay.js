#!/usr/bin/env node
/* global __dirname, process, require -- Node.js runtime */

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { absolutePath, makeDir, under, writeText } = require('./helpers/sandbox');
const isolateWorkingDirectory = require('./fixtures/working-directory');
const { execFileSync } = require('node:child_process');

const REPLAY = under(__dirname, '..', 'benchmarks', 'replay-compress.js');

isolateWorkingDirectory();

function output(lines) {
  return Array.from({ length: lines }, (_, i) => `line ${i} ${'x'.repeat(80)}`).join('\n');
}

test('Pi replay reads toolResult messages and reports marginal post-truncation savings', () => {
  const dir = absolutePath('sessions');
  makeDir('sessions');
  const entries = [
    { type: 'session', version: 3, id: 'session', cwd: '/tmp' },
    { type: 'message', id: '1', parentId: null, message: {
      role: 'toolResult', toolName: 'bash', toolCallId: 'call-1',
      content: [{ type: 'text', text: output(500) }], isError: false,
    } },
    { type: 'message', id: '2', parentId: '1', message: {
      role: 'toolResult', toolName: 'read', toolCallId: 'call-2',
      content: [{ type: 'text', text: output(500) }], isError: false,
    } },
  ];
  writeText(under(dir, 'session.jsonl'), entries.map((entry) => JSON.stringify(entry)).join('\n'));

  const result = execFileSync(process.execPath, [REPLAY, 'pi', dir], { encoding: 'utf8' });
  assert.match(result, /replay \(pi\)/);
  assert.match(result, /marginal savings/);
  assert.match(result, /tool_results scanned:\s+2/);
  assert.match(result, /per-tool breakdown \(incl\. dedup\):/);
  assert.match(result, /bash\s+\d+ outputs\s+[\d,]+ → [\d,]+ chars\s+saved [\d,]+ chars/);
  assert.doesNotMatch(
    result.match(/per-tool breakdown \(incl\. dedup\):\n([\s\S]*?)(?:\n\nbig outputs NOT touched|$)/)?.[1] || '',
    /read/
  );
  assert.match(result, /read\s+[\d,]+ chars/);
  assert.doesNotMatch(result, /saved:\s+0 chars/);
});

test('Claude replay never counts a failed tool call as compressible', () => {
  const dir = absolutePath('sessions');
  makeDir('sessions');
  const use = (id) => ({ message: { role: 'assistant', content: [{ type: 'tool_use', id, name: 'Bash', input: {} }] } });
  const result = (id, isError) => ({ message: { role: 'user', content: [
    { type: 'tool_result', tool_use_id: id, is_error: isError, content: output(500) },
  ] } });
  writeText(under(dir, 'session.jsonl'), [use('ok'), result('ok', false), use('bad'), result('bad', true)].map((entry) => JSON.stringify(entry)).join('\n'));

  const out = execFileSync(process.execPath, [REPLAY, 'claude', dir], { encoding: 'utf8' });
  assert.match(out, /tool_results scanned:\s+2/);
  assert.match(out, /scrubbed\/elided outputs:\s+1 /);
  assert.match(out, /big failed outputs, unreachable: 1 /);
});

test('Claude replay reports per-tool compression savings', () => {
  const dir = absolutePath('sessions');
  makeDir('sessions');

  const use = (id, name) => ({
    message: {
      role: 'assistant',
      content: [{ type: 'tool_use', id, name, input: {} }],
    },
  });

  const result = (id) => ({
    message: {
      role: 'user',
      content: [{
        type: 'tool_result',
        tool_use_id: id,
        is_error: false,
        content: output(500),
      }],
    },
  });

  writeText(under(dir, 'session.jsonl'), [
    use('bash-1', 'Bash'),
    result('bash-1'),
    use('bash-2', 'Bash'),
    result('bash-2'),
    use('grep-1', 'Grep'),
    result('grep-1'),
  ].map(message => JSON.stringify(message)).join('\n'));

  const out = execFileSync(
    process.execPath,
    [REPLAY, 'claude', dir],
    { encoding: 'utf8', env: { ...process.env, PURITANICAL_COMPRESS_DEDUP: '1' } }
  );

  assert.match(out, /per-tool breakdown \(incl\. dedup\):/);
  assert.match(out, /deduped repeat outputs:\s+1 /);
  assert.match(out, /Bash\s+2 outputs\s+[\d,]+ → [\d,]+ chars\s+saved [\d,]+ chars/);
  assert.match(out, /Grep\s+\d+ outputs\s+[\d,]+ → [\d,]+ chars\s+saved [\d,]+ chars/);
});
