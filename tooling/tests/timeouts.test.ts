import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import type { TestEvent } from 'node:test/reporters';
import failures, { cancelled } from '../ci/timeouts.js';

// Events as node's test runner reports them: a file its timeout cancelled fails under its own
// name, at the file's whole time; a test that fails is named for itself, in its file. Either
// error wraps what was thrown, and says which kind of failure it was.
const passed = (file: string): TestEvent => ({
  type: 'test:pass',
  data: {
    name: 'passes',
    nesting: 0,
    testNumber: 1,
    file: path.resolve(file),
    details: { duration_ms: 5 },
  },
});
const failed = (file: string, name: string, failureType: string): TestEvent => ({
  type: 'test:fail',
  data: {
    name,
    nesting: 0,
    testNumber: 1,
    file: path.resolve(file),
    details: {
      duration_ms: 60_000,
      error: Object.assign(new Error(failureType), { failureType, cause: new Error(name) }),
    },
  },
});
const timedOut = (file: string) => failed(file, file, 'testTimeoutFailure');
const broken = (file: string) => failed(file, 'asserts', 'testCodeFailure');

async function* reported(events: TestEvent[]) {
  yield* events;
}
/** The files `check:rules` runs again alone after a run that reported `events`. */
async function runAgain(...events: TestEvent[]) {
  let record = '';
  for await (const line of failures(reported(events))) record += line;
  return cancelled(record);
}

test('the rule test files the timeout cancelled run again alone, and a failure runs nothing again', async () => {
  assert.deepEqual(await runAgain(passed('a.test.ts')), []);
  assert.deepEqual(
    await runAgain(
      timedOut('app/tests/rules/a.test.ts'),
      passed('b.test.ts'),
      timedOut('c.test.ts'),
    ),
    ['app/tests/rules/a.test.ts', 'c.test.ts'],
  );
  // A failure fails the run as it is, beside a timeout too: it never gets a second try.
  assert.deepEqual(await runAgain(broken('a.test.ts')), []);
  assert.deepEqual(await runAgain(timedOut('a.test.ts'), broken('b.test.ts')), []);
  assert.deepEqual(await runAgain(broken('a.test.ts'), timedOut('a.test.ts')), []);
});
