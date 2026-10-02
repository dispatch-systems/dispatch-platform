import { spawnSync } from 'node:child_process';
import os from 'node:os';
import { nodeTests } from '../ci/execution-plan.js';

const args = process.argv.slice(2);
const command = nodeTests('all', {
  // Every test file owns its servers, ports, state and mail, so files run in parallel.
  concurrency: os.availableParallelism(),
  args: args.filter((arg) => arg !== '--list'),
});
// Print the actual command without executing the suite (or its native/browser fixtures).
if (args.includes('--list')) process.stdout.write(`${JSON.stringify([command])}\n`);
else {
  const result = spawnSync(command.command, command.args, { stdio: 'inherit' });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
}
