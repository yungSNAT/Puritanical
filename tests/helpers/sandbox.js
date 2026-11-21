/* global __dirname, module, process, require -- Node.js runtime */
'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const REPO_ROOT = under(__dirname, '..', '..');

function under(base, ...parts) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return path.join(base, ...parts);
}

function absolutePath(...parts) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return path.resolve(...parts);
}

function repoPath(...parts) {
  return under(REPO_ROOT, ...parts);
}

function tempRoot(t, prefix) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  const root = fs.mkdtempSync(under(os.tmpdir(), prefix));
  t.after(() => remove(root, { recursive: true, force: true }));
  return root;
}

function readText(filename) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return fs.readFileSync(filename, 'utf8'); // eslint-disable-line security/detect-non-literal-fs-filename -- Test-only helper; paths come from os.tmpdir() fixtures or the repository checkout.
}

function writeText(filename, text, options) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  fs.writeFileSync(filename, text, options); // eslint-disable-line security/detect-non-literal-fs-filename -- Test-only helper; paths come from os.tmpdir() fixtures or the repository checkout.
}

function readJson(filename) {
  return JSON.parse(readText(filename));
}

function writeJson(filename, value) {
  writeText(filename, JSON.stringify(value));
}

function exists(filename) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return fs.existsSync(filename); // eslint-disable-line security/detect-non-literal-fs-filename -- Test-only helper; paths come from os.tmpdir() fixtures or the repository checkout.
}

function makeDir(dirname, options) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return fs.mkdirSync(dirname, options); // eslint-disable-line security/detect-non-literal-fs-filename -- Test-only helper; paths come from os.tmpdir() fixtures or the repository checkout.
}

function listDir(dirname) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return fs.readdirSync(dirname); // eslint-disable-line security/detect-non-literal-fs-filename -- Test-only helper; paths come from os.tmpdir() fixtures or the repository checkout.
}

function remove(filename, options) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  fs.rmSync(filename, options);
}

function writeStdoutSync(text) {
  fs.writeSync(1, text);
}

function writeStderrSync(text) {
  fs.writeSync(2, text);
}

function clearModule(absPath) {
  delete require.cache[require.resolve(absPath)];
}

function loadModule(absPath) {
  // nosemgrep -- test-only helper; paths come from os.tmpdir() fixtures or the repository checkout, never from user input
  return require(absPath); // eslint-disable-line security/detect-non-literal-require, security-node/detect-non-literal-require-calls -- Test-only helper; loads modules from the repository checkout.
}

function loadFresh(absPath) {
  clearModule(absPath);
  return loadModule(absPath);
}

function saveEnvironment(keys) {
  return Object.fromEntries(keys.map(key => [key, process.env[key]])); // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
}

function restoreEnvironment(saved) {
  for (const [key, value] of Object.entries(saved)) {
    if (value === undefined) delete process.env[key]; // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
    else process.env[key] = value; // eslint-disable-line security/detect-object-injection -- Keys come from the test's fixed list of environment variable names.
  }
}

function isolatedCompressionEnv(home) {
  const env = {
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    XDG_CONFIG_HOME: under(home, 'xdg'),
    CHISLE_STATE_DIR: under(home, 'state'),
    CHISLE_COMPRESS_MAX_CHARS: '8000',
  };
  delete env.CHISLE_COMPRESS;
  delete env.CHISLE_COMPRESS_TOOLS;
  delete env.PI_CODING_AGENT_SESSION_DIR;
  delete env.PI_CODING_AGENT_DIR;
  return env;
}

module.exports = {
  fs,
  under,
  absolutePath,
  repoPath,
  tempRoot,
  readText,
  writeText,
  readJson,
  writeJson,
  exists,
  makeDir,
  listDir,
  remove,
  writeStdoutSync,
  writeStderrSync,
  clearModule,
  loadModule,
  loadFresh,
  saveEnvironment,
  restoreEnvironment,
  isolatedCompressionEnv,
};
