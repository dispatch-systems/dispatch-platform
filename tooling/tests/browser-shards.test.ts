import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { browserTests, shards } from '../ci/browser-shards.js';
import { workflowField, workflowNumbers } from '../testing/workflow.js';

test('the browser shards split every test into exactly one shard by recorded time', () => {
  const times = Object.fromEntries(
    Array.from({ length: 40 }, (_, index) => [
      `t${index}.spec.ts › test ${index}`,
      (index * 7) % 23,
    ]),
  );
  const tests = [...Object.keys(times), 'new.spec.ts › not yet timed'];
  // An untimed test counts as the median recorded time.
  const median = Object.values(times).sort((a, b) => a - b)[20]!;
  for (let count = 1; count <= 8; count += 1) {
    const split = shards(tests, count, times);
    assert.equal(split.length, count);
    assert.deepEqual(split.flat().sort(), [...tests].sort());
    // Longest first to the lightest shard: no shard ends more than one test ahead of another.
    const loads = split.map((shard) =>
      shard.reduce((sum, name) => sum + (times[name] ?? median), 0),
    );
    assert(Math.max(...loads) - Math.min(...loads) <= 22, `${count}: ${loads.join(' ')}`);
    // Every shard computes the same split, whatever order the listing came in.
    assert.deepEqual(shards([...tests].reverse(), count, times), split);
  }
});

test('the recorded times name the suite, and the workflow runs as many shards as it splits', () => {
  const tests = browserTests();
  assert(tests.length > 50);
  const times = JSON.parse(fs.readFileSync('tooling/ci/browser-durations.json', 'utf8'));
  assert.deepEqual(
    Object.keys(times).filter((name) => !tests.includes(name)),
    [],
    'remove stale recorded titles',
  );
  const timed = tests.filter((name) => name in times);
  assert(timed.length >= tests.length * 0.9, 'refresh with tooling/ci/browser-durations.py');
  const workflow = fs.readFileSync('.github/workflows/checks.yml', 'utf8');
  const browser = workflowField(workflow, 'jobs', 'browser').body;
  const matrix = workflowNumbers(workflowField(browser, 'strategy', 'matrix', 'shard'));
  const count = Number(
    /check:ci\s+--\s+browser\s+\$\{\{\s*matrix\.shard\s*\}\}\/(\d+)/.exec(browser)?.[1],
  );
  assert(count > 0);
  assert.deepEqual(
    matrix,
    Array.from({ length: count }, (_, index) => index + 1),
  );
  assert.deepEqual(shards(tests, count).flat().sort(), [...tests].sort());
  // YAML's block/flow sequence choices and legal indentation do not change coverage.
  for (const source of [
    'jobs:\n browser:\n  strategy:\n   matrix:\n    shard: [1,2,3]\n',
    'jobs:\n    browser:\n        strategy:\n            matrix:\n                shard:\n                    - 1\n                    - 2\n                    - 3\n',
  ])
    assert.deepEqual(
      workflowNumbers(workflowField(source, 'jobs', 'browser', 'strategy', 'matrix', 'shard')),
      [1, 2, 3],
    );
});
