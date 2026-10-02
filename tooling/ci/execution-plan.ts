import { allTests, coreTests, dashboardTests, ruleTests, sourceLints } from './test-plan.js';

export type Command = {
  name: string;
  command: string;
  args: string[];
  /** Overrides only; callers merge these with the environment instead of printing secrets. */
  env?: NodeJS.ProcessEnv;
};

/** The commands below are consumed by both the runners and their dry-run coverage tests. */
export function nodeTests(
  suite: 'all' | 'core' | 'dashboard' | 'rules',
  options: { concurrency?: number; args?: string[]; env?: NodeJS.ProcessEnv } = {},
): Command {
  const files = {
    all: allTests,
    core: coreTests,
    dashboard: () => dashboardTests,
    rules: () => ruleTests,
  }[suite]();
  return {
    name: `${suite} tests`,
    command: process.execPath,
    args: [
      'node_modules/tsx/dist/cli.mjs',
      '--test',
      ...(options.concurrency === undefined ? [] : [`--test-concurrency=${options.concurrency}`]),
      ...(options.args ?? []),
      ...files,
    ],
    ...(options.env ? { env: options.env } : {}),
  };
}

export const pythonTests: Command = {
  name: 'Python tooling tests',
  command: 'python3',
  args: ['-m', 'unittest', 'discover', '-s', 'tests/tooling', '-p', '*_test.py'],
};

export const sourceLintCommands: Command[] = sourceLints.map((file) => ({
  name: file,
  command: process.execPath,
  args: ['node_modules/tsx/dist/cli.mjs', file],
}));
