import { spawn } from 'node:child_process';
import { ruleTests } from './test-plan.js';

// Everything a push can fail on without a build: types, formatting, the source-wide rule
// tests and the Python tooling tests, which take seconds and otherwise fail only in the queue.
// About half a minute; run it before every push.
const checks: [string, string, string[]][] = [
  ['privacy', 'python3', ['tooling/security/scan.py']],
  [
    'Python tooling tests',
    'python3',
    ['-m', 'unittest', 'discover', '-s', 'tests/tooling', '-p', '*_test.py'],
  ],
  ['types', 'npx', ['tsc', '--noEmit']],
  ['format', 'npx', ['prettier', '--check', '.']],
  ['Rust format', 'cargo', ['fmt', '--check']],
  ['rule tests', process.execPath, ['node_modules/tsx/dist/cli.mjs', '--test', ...ruleTests]],
];
const results = await Promise.all(
  checks.map(
    ([name, command, args]) =>
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
