import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { TestEvent } from 'node:test/reporters';
import { nodeTests } from '../ci/execution-plan.js';
import failures, { cancelled } from '../ci/timeouts.js';

// Events as Node 26's runner reports them, taken from a run of a file whose three tests pass,
// assert wrongly and hang: each test under its own name, in its file, a timed-out one too. A
// failure's error is the runner's, saying which kind of failure it was, its cause what the test
// threw or, for a timeout, the timeout's message.
/** The `testNumber`th test of `file`, named `name`, at `line`. */
const located = (file: string, name: string, testNumber: number, line: number) => ({
  name,
  nesting: 0,
  testNumber,
  testId: testNumber,
  parentId: 0,
  tags: [],
  line,
  column: 1,
  file: path.resolve(file),
  entryFile: path.resolve(file),
});
const passed = (file: string): TestEvent => ({
  type: 'test:pass',
  data: { ...located(file, 'passes', 1, 3), details: { duration_ms: 1.57, type: 'test' } },
});
/** The test `at` failed after `duration_ms`, of the kind `failureType`, ended by `cause`. */
const failed = (
  at: ReturnType<typeof located>,
  duration_ms: number,
  failureType: string,
  cause: Error | string,
): TestEvent => ({
  type: 'test:fail',
  data: {
    ...at,
    details: {
      duration_ms,
      type: 'test',
      error: Object.assign(new Error(typeof cause === 'string' ? cause : cause.message), {
        code: 'ERR_TEST_FAILURE',
        failureType,
        cause,
      }),
    },
  },
});
const broken = (file: string) =>
  failed(
    located(file, 'asserts', 2, 4),
    2.73,
    'testCodeFailure',
    new assert.AssertionError({ actual: 1, expected: 2, operator: 'strictEqual' }),
  );
const timedOut = (file: string) =>
  failed(
    located(file, 'hangs', 3, 5),
    60_000.36,
    'testTimeoutFailure',
    'test timed out after 60000ms',
  );

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

/** A test file whose one test outlives its timeout while it holds a timer open. */
const holdsOpen = 'tooling/tests/support/times-out-holding-a-timer.ts';

test('a test file whose test timed out ends though it holds a timer open, and is picked to run again', async (t) => {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-timeouts-'));
  t.after(() => fs.rmSync(scratch, { recursive: true, force: true }));
  const record = path.join(scratch, 'failures.jsonl');
  // The rule tests' run as `check:rules` starts it, recording its failures, with none of the
  // suite's files but that one alone, and a timeout it reaches at once.
  const { command, args } = nodeTests('rules', {
    only: [],
    args: [
      '--test-timeout=1',
      '--test-reporter=./tooling/ci/timeouts.ts',
      `--test-reporter-destination=${record}`,
      holdsOpen,
    ],
  });
  // A run of its own, not one reporting to this run, in a process group of its own.
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  const run = spawn(command, args, { env, detached: true, stdio: 'ignore' });
  const ended = new Promise((resolve) => run.once('exit', resolve));
  // Should the file never end, the suite's timeout fails this test and ends all the run started.
  const end = () => process.kill(-run.pid!, 'SIGKILL');
  t.signal.addEventListener('abort', end);
  const code = await ended;
  t.signal.removeEventListener('abort', end);
  assert.equal(code, 1);
  assert.deepEqual(cancelled(fs.readFileSync(record, 'utf8')), [holdsOpen]);
});
