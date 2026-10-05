import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { overBudget, programs } from '../ci/test-build-budget.js';

test('only the test programs Cargo built count toward the budget', () => {
  const deps = fs.mkdtempSync(path.join(os.tmpdir(), 'test-build-budget-'));
  try {
    const write = (name: string, bytes: number, mode: number) => {
      fs.writeFileSync(path.join(deps, name), Buffer.alloc(bytes));
      fs.chmodSync(path.join(deps, name), mode);
    };
    write('dispatch_core-0123456789abcdef', 300, 0o755);
    write('agent_api-fedcba9876543210', 200, 0o755);
    write('libdispatch_core-0123456789abcdef.rlib', 900, 0o644);
    write('libserde_derive-0123456789abcdef.so', 900, 0o755);
    write('dispatch_core-0123456789abcdef.d', 50, 0o644);
    fs.mkdirSync(path.join(deps, 'nested-0123456789abcdef'));
    assert.deepEqual(
      programs(deps).sort((a, b) => a.name.localeCompare(b.name)),
      [
        { name: 'agent_api-fedcba9876543210', bytes: 200 },
        { name: 'dispatch_core-0123456789abcdef', bytes: 300 },
      ],
    );
  } finally {
    fs.rmSync(deps, { recursive: true, force: true });
  }
});

test('a program over its own budget, or all of them over theirs, fails the build', () => {
  const limits = { programBytes: 100 * 1024 ** 2, totalBytes: 250 * 1024 ** 2 };
  const program = (name: string, megabytes: number) => ({ name, bytes: megabytes * 1024 ** 2 });
  assert.deepEqual(overBudget([program('a', 90), program('b', 90)], limits), []);
  assert.deepEqual(overBudget([program('a', 90), program('b', 130)], limits), [
    'b is 130 MB, over the 100 MB a test program may take.',
  ]);
  assert.deepEqual(overBudget([program('a', 90), program('b', 90), program('c', 90)], limits), [
    'The 3 test programs take 270 MB, over the 250 MB they may take together.',
  ]);
});
