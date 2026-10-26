import { spawn } from 'node:child_process';
import { nodeTests, pythonTests, sourceLintCommands, type Command } from './execution-plan.js';

// Everything a push can fail on without a build: types, formatting, the source-wide rule
// tests and source-only Python tooling tests. Real compiler and installed-manager checks
// run in the API job; this command never builds Rust.
// About a minute; `ci` and `ship` run it before a push. TypeScript and Prettier keep what they
// found in the worktree's node_modules, so a later run checks again only what changed; CI's
// checks job runs them afresh.
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
  nodeTests('rules'),
];
if (process.argv.includes('--list')) {
  process.stdout.write(`${JSON.stringify(checks)}\n`);
  process.exit(0);
}
const results = await Promise.all(
  checks.map(
    ({ name, command, args, env }) =>
      new Promise<string | undefined>((resolve) => {
        const child = spawn(command, args, { stdio: 'inherit', env: { ...process.env, ...env } });
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
