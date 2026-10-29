import path from 'node:path';
import type { TestEvent } from 'node:test/reporters';

// What the rule tests' timeout cancelled. It guards against a hung test, but a machine busy with
// other worktrees' builds can slow a test past it too, so `check:rules` runs the files with a test
// it cancelled again, alone, once its other checks have ended. This reporter records each failure
// beside the run's own output, and `cancelled` reads which files those are.

/**
 * Node's name for the failure a test's timeout made. A hook's timeout fails as the hook does
 * (`hookFailed`), so a file whose hook it cancelled runs nothing again.
 */
const TIMEOUT = 'testTimeoutFailure';

/** A node test reporter writing each failure as a line of JSON: its file, and the kind node gave it. */
export default async function* failures(events: AsyncIterable<TestEvent>) {
  for await (const event of events) {
    if (event.type !== 'test:fail') continue;
    const { file, details } = event.data;
    const { failureType } = details.error as { failureType?: string };
    yield `${JSON.stringify({ file: file && path.relative(process.cwd(), file), failureType })}\n`;
  }
}

/**
 * The files to run again alone, from what `failures` recorded: those with a test the timeout
 * cancelled, when that is all that failed. Any other failure fails the run as it is, and runs
 * nothing again. So does a nested test's timeout, which fails beside it its suite
 * (`subtestsFailed`), or the subtest its parent's timeout cut short (`cancelledByParent`); no rule
 * test nests.
 */
export function cancelled(record: string): string[] {
  const recorded = record
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { file?: string; failureType?: string });
  const timedOut = recorded.flatMap(({ file, failureType }) =>
    file && failureType === TIMEOUT ? [file] : [],
  );
  return timedOut.length === recorded.length ? [...new Set(timedOut)] : [];
}
