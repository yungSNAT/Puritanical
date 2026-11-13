// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: FSL-1.1-ALv2

// writeDefaultMode rewrites config.json, which holds the user's other settings:
// a failed write must leave the old file whole, and a symlink there is refused.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { writeDefaultMode } = require('../hooks/puritanical-config');

const ORIGINAL = JSON.stringify({ defaultMode: 'lite', hideStatus: true });

function configHome(t) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-write-'));
  const previous = process.env.XDG_CONFIG_HOME;
  process.env.XDG_CONFIG_HOME = home;
  t.after(() => {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME;
    else process.env.XDG_CONFIG_HOME = previous;
    fs.rmSync(home, { recursive: true, force: true });
  });
  const dir = path.join(home, 'puritanical');
  fs.mkdirSync(dir);
  return { dir, config: path.join(dir, 'config.json') };
}

const diskFull = () => Object.assign(new Error('no space left on device'), { code: 'ENOSPC' });

test('a write that fails on a full disk keeps the old config and leaves no temp file', (t) => {
  const { dir, config } = configHome(t);
  fs.writeFileSync(config, ORIGINAL);
  // A real full disk fails after the file is opened for writing, so open (and so
  // truncate, for a write in place) whatever path is named, then fail.
  t.mock.method(fs, 'writeFileSync', (file) => {
    if (typeof file !== 'number') fs.closeSync(fs.openSync(file, 'w'));
    throw diskFull();
  });
  assert.throws(() => writeDefaultMode('ultra'), { code: 'ENOSPC' });
  t.mock.restoreAll();
  assert.equal(fs.readFileSync(config, 'utf8'), ORIGINAL);
  assert.deepEqual(fs.readdirSync(dir), ['config.json']);
});

test('a failed rename keeps the old config and leaves no temp file', (t) => {
  const { dir, config } = configHome(t);
  fs.writeFileSync(config, ORIGINAL);
  t.mock.method(fs, 'renameSync', () => { throw Object.assign(new Error('busy'), { code: 'EPERM' }); });
  assert.throws(() => writeDefaultMode('ultra'), { code: 'EPERM' });
  t.mock.restoreAll();
  assert.equal(fs.readFileSync(config, 'utf8'), ORIGINAL);
  assert.deepEqual(fs.readdirSync(dir), ['config.json']);
});

test('a successful write replaces the config, keeping its other keys and its mode', (t) => {
  const { dir, config } = configHome(t);
  fs.writeFileSync(config, ORIGINAL);
  if (process.platform !== 'win32') fs.chmodSync(config, 0o600);
  assert.equal(writeDefaultMode('ultra'), 'ultra');
  assert.deepEqual(JSON.parse(fs.readFileSync(config, 'utf8')), { defaultMode: 'ultra', hideStatus: true });
  if (process.platform !== 'win32') assert.equal(fs.statSync(config).mode & 0o777, 0o600);
  assert.deepEqual(fs.readdirSync(dir), ['config.json']);
});

test('a symlinked config is refused, not written through or replaced', (t) => {
  const { dir, config } = configHome(t);
  const victim = path.join(dir, 'victim.json');
  fs.writeFileSync(victim, ORIGINAL);
  try { fs.symlinkSync(victim, config); } catch (e) {
    if (e.code !== 'EPERM') throw e;
    t.skip('symlink creation is not permitted');
    return;
  }
  assert.throws(() => writeDefaultMode('ultra'), /symlink/);
  assert.ok(fs.lstatSync(config).isSymbolicLink());
  assert.equal(fs.readFileSync(victim, 'utf8'), ORIGINAL);
  assert.deepEqual(fs.readdirSync(dir).sort(), ['config.json', 'victim.json']);
});
