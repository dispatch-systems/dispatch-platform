import test from 'node:test';

// A test file `timeouts.test.ts` runs: its one test never ends and holds a timer open, so only
// the test's timeout ends it, and only the runner ends its file's process.
test('holds a timer open past its timeout', () => new Promise(() => setInterval(() => {}, 1000)));
