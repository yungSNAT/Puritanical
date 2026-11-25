/* global process, require -- Node.js runtime */
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const {
  exists, isolatedCompressionEnv, makeDir, repoPath, tempRoot, under, writeJson,
} = require('./helpers/sandbox');
const coreWasm = require('../hooks/puritanical-core-wasm.cjs');

test('puritanical-core-wasm handles edge arguments, surrogates, and custom headings', () => {
  assert.equal(coreWasm.requestedMode(''), null);
  assert.equal(coreWasm.requestedMode('puritanical on'), 'on');
  assert.equal(coreWasm.requestedMode('puritanical off'), 'off');
  assert.equal(coreWasm.requestedMode('everyday prose'), null);

  assert.equal(coreWasm.filterSections('', true, false), '');
  assert.equal(
    coreWasm.filterSections('Pre\n## P\nbody\n## K\nkeep', false, true, ['P'], ['C']),
    'Pre\n## K\nkeep',
  );
  assert.equal(coreWasm.isOldStatusLineCommand('bash "/x/chisle-statusline.sh"'), true);
  assert.equal(coreWasm.isOwnedStatusLineCommand('bash "/x/puritanical-statusline.sh"'), true);
  assert.equal(coreWasm.isOwnedStatusLineCommand('echo hi'), false);

  assert.equal(coreWasm.unsafeToolName('read_file'), true);
  assert.equal(coreWasm.unsafeToolName('Bash'), false);
  assert.equal(coreWasm.containsCredential('github_pat_' + 'a'.repeat(22)), true);
  assert.equal(coreWasm.containsCredential('safe text'), false);

  assert.equal(coreWasm.scrub('line\r\n\r\n\r\n  \nend'), 'line\r\n\r\n\r\n\nend');
  assert.match(coreWasm.duplicateMarker('Bash', 'a\nb', 'earlier'), /an earlier Bash result/);
  assert.equal(coreWasm.compress('short', { maxChars: 100, headLines: 5, tailLines: 5 }, null), null);
  assert.equal(coreWasm.boundForOpencode('tiny', null, 0), 'tiny');
  assert.equal(coreWasm.boundForOpencode('tiny', null, Infinity), 'tiny');
  assert.equal(coreWasm.boundForOpencode('a\uD83D\uDE00b\uD83D\uDE00c', null, 4), 'a\uD83D\uDE00b');

  assert.equal(coreWasm.envAssignEnd('A=1 cmd', undefined), 4);
  assert.deepEqual([...coreWasm.kindsFor('git status')], ['git-status']);
  assert.equal(coreWasm.kindsFor('echo ok'), null);
  assert.equal(coreWasm.wrappable('cargo test'), true);
  assert.equal(coreWasm.filterCommand('cat a', 'hi', null), null);
});

test('installers copy puritanical-core-wasm.cjs and puritanical-core.wasm to all hook targets', (t) => {
  const home = tempRoot(t, 'puritanical-wasm-install-');
  const claudeDir = under(home, '.claude');
  const copilotDir = under(home, '.copilot');
  const opencodeDir = under(home, '.config', 'opencode');
  const binDir = under(home, 'empty-bin');
  makeDir(binDir, { recursive: true });
  makeDir(opencodeDir, { recursive: true });
  makeDir(copilotDir, { recursive: true });

  const env = {
    ...isolatedCompressionEnv(home),
    PATH: binDir,
    HOME: home,
    USERPROFILE: home,
    PURITANICAL_HOME: home,
    CLAUDE_CONFIG_DIR: claudeDir,
    COPILOT_HOME: copilotDir,
    XDG_CONFIG_HOME: under(home, '.config'),
    OPENCODE_CONFIG_DIR: opencodeDir,
  };

  execFileSync(process.execPath, [
    repoPath('bin', 'install.js'), '--only', 'claude', '--config-dir', claudeDir,
  ], { env, encoding: 'utf8' });
  assert.ok(exists(under(claudeDir, 'puritanical-hooks', 'puritanical-core-wasm.cjs')));
  assert.ok(exists(under(claudeDir, 'puritanical-hooks', 'puritanical-core.wasm')));

  execFileSync(process.execPath, [
    repoPath('bin', 'install.js'), '--only', 'opencode',
  ], { env, encoding: 'utf8' });
  assert.ok(exists(under(opencodeDir, 'plugins', 'puritanical-hooks', 'puritanical-core-wasm.cjs'))
    || exists(under(opencodeDir, 'plugins', 'puritanical', 'puritanical-hooks', 'puritanical-core-wasm.cjs')));

  execFileSync(process.execPath, [
    repoPath('bin', 'install.js'), '--only', 'copilot-cli',
  ], { env, encoding: 'utf8' });
  assert.ok(exists(under(copilotDir, 'puritanical-hooks', 'puritanical-core-wasm.cjs')));
  assert.ok(exists(under(copilotDir, 'puritanical-hooks', 'puritanical-core.wasm')));

  const proxyDir = under(home, '.config', 'puritanical', 'mcp-proxy');
  const mcpTarget = under(home, '.cursor', 'mcp.json');
  makeDir(under(home, '.cursor'), { recursive: true });
  writeJson(mcpTarget, { mcpServers: { s: { command: 'node', args: ['-v'] } } });
  execFileSync(process.execPath, [
    repoPath('bin', 'install.js'), 'mcp', 'wrap', '--file', mcpTarget,
  ], { env, encoding: 'utf8' });
  assert.ok(exists(under(proxyDir, 'puritanical-core-wasm.cjs')));
  assert.ok(exists(under(proxyDir, 'puritanical-core.wasm')));
});
