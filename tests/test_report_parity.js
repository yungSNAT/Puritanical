// The hook's side of `puritanical-tools report` parity: these sizes are pinned
// in `tools/tests/report.rs` (`sizes_match_the_hooks_transform`) too, so a
// change to either side's thresholds or tiers fails one of the 2 tests.
const test = require('node:test');
const assert = require('node:assert/strict');
const { transform, THRESHOLDS } = require('../hooks/puritanical-compress-output.js');

const size = (text) => (transform(text, THRESHOLDS, null, null, null) ?? text).length;

function padded(n, spaces) {
  let s = 'x'.repeat(1100) + '\n';
  for (let i = 0; i < n; i++) s += `y${i}${' '.repeat(spaces)}\n`;
  return s;
}

test('report fixtures compress to the sizes the Rust report pins', () => {
  let log = '';
  for (let i = 0; i < 300; i++) log += `step ${String(i).padStart(3, '0')}: compiling module number ${i} of the build\n`;
  assert.equal(size(log), 5044);
  assert.equal(transform(padded(21, 3), THRESHOLDS, null, null, null), null);
  assert.equal(size(padded(16, 4)), 1155);
  let wide = '';
  for (let i = 0; i < 50; i++) wide += `${i}:${'abcdefghij'.repeat(30)}\n`;
  assert.equal(size(wide), 8060);
});
