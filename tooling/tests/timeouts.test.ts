import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { TestEvent } from 'node:test/reporters';
import { nodeTests } from '../ci/execution-plan.js';
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
    testId: 1,
    parentId: 0,
    tags: [],
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
    testId: 1,
    parentId: 0,
    tags: [],
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

/** A test file whose one test outlives its timeout while it holds a timer open. */
const holdsOpen = 'tooling/tests/support/times-out-holding-a-timer.ts';

test('a test file whose test timed out ends though it holds a timer open, and runs again alone', async (t) => {
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
