/* global process, require, module -- Node.js runtime */
'use strict';

const { beforeEach } = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

// node --test gives each suite its own process. Each serial test gets a private
// working directory, so a failed assertion still cleans up its fixture tree.
module.exports = function useTemporaryWorkingDirectory() {
  beforeEach(t => {
    const previous = process.cwd();
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-test-'));
    process.chdir(root);
    t.after(() => {
      process.chdir(previous);
      fs.rmSync(root, { recursive: true, force: true });
    });
  });
};
