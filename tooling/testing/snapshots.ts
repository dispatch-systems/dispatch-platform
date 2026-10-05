import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Command } from '../ci/execution-plan.js';

// `npm run snapshots:update`: rewrites the snapshots the app's tests hold the whole product to,
// such as its catalog, its databases and their schemas, the agent API and the kinds of data
// agents read, then names each file that changed, to review with the change that changed it.
// Each of those tests fails on a difference, naming this command, unless
// DISPATCH_UPDATE_SNAPSHOTS is set, as it is here. `--list` prints the commands and runs nothing.
//
// Some of the tests read a snapshot another writes, and they run at once, so after the build a
// first pass writes the snapshots and a second, without the variable, runs every test against
// them.

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
/** Where the snapshots are. */
const snapshotDirs = [
  'app/tests/backend/snapshots',
  'app/tests/backend/agent_api',
  'app/tests/frontend/snapshots',
  'core/db/tests/backend/schema',
];
const frontend = fs
  .readdirSync(path.join(root, 'app/tests/frontend'))
  .filter((name) => name.endsWith('.test.ts'))
  .map((name) => `app/tests/frontend/${name}`)
  .sort();
const rust = ['test', '--locked', '-p', 'dispatch-backend', '--lib', '--test', 'agent_api'];
/** The build of the Rust tests, whose output shows once. */
const build: Command = { name: 'build', command: 'cargo', args: [...rust, '--no-run'] };
/**
 * The app's tests that hold snapshots: its Rust module tests and the agent API's, each binary
 * run whether or not another fails, and its frontend's.
 */
const commands: Command[] = [
  { name: 'Rust', command: 'cargo', args: [...rust, '--no-fail-fast'] },
  {
    name: 'frontend',
    command: process.execPath,
    args: ['node_modules/tsx/dist/cli.mjs', '--test', ...frontend],
  },
];

/** Every snapshot file, by its path from the root, with what it holds. */
function snapshots() {
  const found = new Map<string, string>();
  for (const dir of snapshotDirs) {
    if (!fs.existsSync(path.join(root, dir))) continue;
    for (const name of fs.readdirSync(path.join(root, dir)).sort()) {
      const file = `${dir}/${name}`;
      if (fs.statSync(path.join(root, file)).isFile())
        found.set(file, fs.readFileSync(path.join(root, file), 'utf8'));
    }
  }
  return found;
}

if (process.argv.includes('--list')) {
  process.stdout.write(`${JSON.stringify([build, ...commands])}\n`);
  process.exit(0);
}
const before = snapshots();
if (spawnSync(build.command, build.args, { cwd: root, stdio: 'inherit' }).status !== 0)
  process.exit(1);
// The first pass's output would only repeat the second's.
const updating = { ...process.env, DISPATCH_UPDATE_SNAPSHOTS: '1' };
for (const { command, args } of commands)
  spawnSync(command, args, { cwd: root, env: updating, stdio: 'ignore' });
const failed = commands.filter(
  ({ command, args }) => spawnSync(command, args, { cwd: root, stdio: 'inherit' }).status !== 0,
);
const after = snapshots();
const changed = [...new Set([...before.keys(), ...after.keys()])]
  .sort()
  .filter((file) => before.get(file) !== after.get(file))
  .map((file) => `  ${file}${!before.has(file) ? ' (new)' : !after.has(file) ? ' (removed)' : ''}`);
process.stdout.write(
  changed.length
    ? `\nSnapshots changed, to review with the change that changed them:\n${changed.join('\n')}\n`
    : '\nNo snapshot changed.\n',
);
if (failed.length) {
  process.stderr.write(
    `Tests still fail with the snapshots rewritten: ${failed.map(({ name }) => name).join(', ')}\n`,
  );
  process.exit(1);
}
