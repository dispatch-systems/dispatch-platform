import { spawn } from 'node:child_process';
import { nodeTests, pythonTests, sourceLintCommands, type Command } from './execution-plan.js';

// Everything a push can fail on without a build: types, formatting, the source-wide rule
// tests and the Python tooling tests, which take seconds and otherwise fail only in the queue.
// About half a minute; run it before every push.
const checks: Command[] = [
  { name: 'privacy', command: 'python3', args: ['tooling/security/scan.py'] },
  pythonTests,
  { name: 'types', command: 'npx', args: ['tsc', '--noEmit'] },
  { name: 'format', command: 'npx', args: ['prettier', '--check', '.'] },
  { name: 'Rust format', command: 'cargo', args: ['fmt', '--check'] },
  ...sourceLintCommands,
  nodeTests('rules'),
];
if (process.argv.includes('--list')) {
  process.stdout.write(`${JSON.stringify(checks)}\n`);
  process.exit(0);
}
const results = await Promise.all(
  checks.map(
    ({ name, command, args }) =>
      new Promise<string | undefined>((resolve) => {
        const child = spawn(command, args, { stdio: 'inherit' });
        child.once('error', () => resolve(name));
        child.once('exit', (code) => resolve(code === 0 ? undefined : name));
      }),
  ),
);
const failed = results.filter(Boolean);
if (failed.length) {
  process.stderr.write(`Failed rules: ${failed.join(', ')}\n`);
  process.exit(1);
}
process.stdout.write('All rules pass.\n');
