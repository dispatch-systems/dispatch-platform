import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { nodeTests, pythonTests, sourceLintCommands, type Command } from './execution-plan.js';
import { cancelled } from './timeouts.js';

// Everything a push can fail on without a build: types, formatting, the source-wide rule
// tests and source-only Python tooling tests. Real compiler and installed-manager checks
// run in the API job; this command never builds Rust.
// About a minute; `ci` and `ship` run it before a push. TypeScript and Prettier keep what they
// found in the worktree's node_modules, so a later run checks again only what changed; CI's
// checks job runs them afresh.

/** Where the rule tests' failures are recorded, for those their timeout cancelled to run again. */
const record = path.join(os.tmpdir(), `dispatch-rule-tests-${process.pid}.jsonl`);
const ruleTests = nodeTests('rules', {
  args: [
    // The reporter node would choose, named since another records the failures beside it.
    `--test-reporter=${process.stdout.isTTY ? 'spec' : 'tap'}`,
    '--test-reporter-destination=stdout',
    '--test-reporter=./tooling/ci/timeouts.ts',
    `--test-reporter-destination=${record}`,
  ],
});
const checks: Command[] = [
  { name: 'privacy', command: 'python3', args: ['tooling/security/scan.py'] },
  pythonTests('rules'),
  {
    name: 'types',
    command: 'npx',
    args: [
      'tsc',
      '--noEmit',
      '--incremental',
      '--tsBuildInfoFile',
      'node_modules/.cache/dispatch/tsc.tsbuildinfo',
    ],
  },
  { name: 'format', command: 'npx', args: ['prettier', '--check', '--cache', '.'] },
  { name: 'Rust format', command: 'cargo', args: ['fmt', '--check'] },
  ...sourceLintCommands,
  ruleTests,
];
if (process.argv.includes('--list')) {
  process.stdout.write(`${JSON.stringify(checks)}\n`);
  process.exit(0);
}
/** Runs `command` with its output here; whether it passed. */
const run = ({ command, args, env }: Command) =>
  new Promise<boolean>((resolve) => {
    const child = spawn(command, args, { stdio: 'inherit', env: { ...process.env, ...env } });
    child.once('error', () => resolve(false));
    child.once('exit', (code) => resolve(code === 0));
  });
const passed = await Promise.all(checks.map(run));
// A busy machine can slow a rule test file past its timeout: once the other checks have ended,
// the files it cancelled run again alone, and only a failure or a second timeout fails them.
const again = fs.existsSync(record) ? cancelled(fs.readFileSync(record, 'utf8')) : [];
fs.rmSync(record, { force: true });
const tests = checks.indexOf(ruleTests);
if (!passed[tests] && again.length) {
  process.stdout.write(
    `Running again alone, as the test timeout cancelled them: ${again.join(', ')}\n`,
  );
  passed[tests] = await run(nodeTests('rules', { concurrency: 1, only: again }));
}
const failed = checks.filter((_, index) => !passed[index]).map(({ name }) => name);
if (failed.length) {
  process.stderr.write(`Failed rules: ${failed.join(', ')}\n`);
  process.exit(1);
}
process.stdout.write('All rules pass.\n');
