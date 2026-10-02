import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { sourceLineViolations } from '../../tooling/ci/source-lines.js';

test('the source lint counts Unicode characters, handles CRLF and ratchets exceptions', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-source-lines-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'nested'));
  fs.writeFileSync(path.join(root, 'nested/valid.rs'), `${'😀'.repeat(140)}\r\n`);
  fs.writeFileSync(path.join(root, 'ignored.txt'), 'x'.repeat(141));
  fs.writeFileSync(path.join(root, 'legacy.rs'), `${'x'.repeat(141)}\n`);
  const exceptions = new Map([['legacy.rs', 1]]);
  assert.deepEqual(sourceLineViolations(root, exceptions), []);
  fs.writeFileSync(path.join(root, 'new.rs'), `${'x'.repeat(140)}\n${'x'.repeat(141)}\n`);
  assert.deepEqual(sourceLineViolations(root, exceptions), [
    'new.rs:2: source lines must fit in 140 characters',
  ]);
  fs.unlinkSync(path.join(root, 'new.rs'));
  fs.writeFileSync(path.join(root, 'legacy.rs'), `${'x'.repeat(140)}\n`);
  assert.match(
    sourceLineViolations(root, exceptions)[0]!,
    /expected 1 .* found 0; lower the budget/,
  );
  fs.writeFileSync(path.join(root, 'legacy.rs'), `${'x'.repeat(141)}\n${'x'.repeat(141)}\n`);
  assert.match(
    sourceLineViolations(root, exceptions)[0]!,
    /expected 1 .* found 2;.*never increase it/,
  );
});
