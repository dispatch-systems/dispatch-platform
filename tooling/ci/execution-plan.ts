import path from 'node:path';
import {
  allTests,
  coreTests,
  dashboardTests,
  ruleTests,
  sourceLints,
  allPythonTests,
  pythonRuleTests,
  pythonIntegrationTests,
  pythonTestDirs,
} from './test-plan.js';

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
  options: {
    concurrency?: number;
    args?: string[];
    env?: NodeJS.ProcessEnv;
    /**
     * Only these of the suite's files, as `check:rules` runs again those with a test its timeout
     * cancelled.
     */
    only?: string[];
  } = {},
): Command {
  const suiteFiles = {
    all: allTests,
    core: coreTests,
    dashboard: () => dashboardTests,
    rules: () => ruleTests,
  }[suite]();
  const files = suiteFiles.filter((file) => !options.only || options.only.includes(file));
  return {
    name: `${suite} tests`,
    command: process.execPath,
    args: [
      'node_modules/tsx/dist/cli.mjs',
      '--test',
      // A hung test fails by name instead of running out its job. The 60 s is each test's and
      // each hook's, and a file's process ends once its tests have, a timed-out one too, whatever
      // it still holds open. Only a test a run opts in to, such as a native one, sets a longer
      // timeout of its own.
      '--test-timeout=60000',
      '--test-force-exit',
      ...(options.concurrency === undefined ? [] : [`--test-concurrency=${options.concurrency}`]),
      ...(options.args ?? []),
      ...files,
    ],
    ...(options.env ? { env: options.env } : {}),
  };
}

export function pythonTests(suite: 'all' | 'rules' | 'integration'): Command {
  const files =
    suite === 'all'
      ? allPythonTests()
      : suite === 'rules'
        ? pythonRuleTests()
        : pythonIntegrationTests;
  return {
    name: `Python ${suite} tests`,
    command: 'python3',
    args: ['-m', 'unittest', ...files.map((file) => path.basename(file, '.py'))],
    env: {
      PYTHONPATH: pythonTestDirs.map((directory) => path.resolve(directory)).join(path.delimiter),
    },
  };
}

export const sourceLintCommands: Command[] = sourceLints.map((file) => ({
  name: file,
  command: process.execPath,
  args: ['node_modules/tsx/dist/cli.mjs', file],
}));
