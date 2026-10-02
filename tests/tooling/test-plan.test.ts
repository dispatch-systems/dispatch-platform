import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import playwrightConfig from '../../playwright.config.js';
import {
  allTests,
  allPythonTests,
  coreTests,
  dashboardTests,
  nativeShards,
  nativeRealTimeout,
  pythonIntegrationTests,
  pythonRuleTests,
  ruleTests,
  sourceLints,
  watchedTests,
} from '../../tooling/ci/test-plan.js';
import { pythonTests, type Command } from '../../tooling/ci/execution-plan.js';
import { readsEnvironment } from '../../tooling/testing/source-analysis.js';
import { workflowField } from '../../tooling/testing/workflow.js';

const names = (directory: string, pattern: RegExp) =>
  fs
    .readdirSync(directory, { recursive: true, encoding: 'utf8' })
    .filter((name) => pattern.test(name))
    .map((name) => `${directory}/${name}`)
    .sort();

// Run the entry points, stopping before execution: these are their actual child commands.
const listed = new Map<string, Command[]>();
const listing = (file: string, args: string[] = []): Command[] => {
  const key = JSON.stringify([file, args]);
  let commands = listed.get(key);
  if (!commands) {
    commands = JSON.parse(
      execFileSync(process.execPath, ['node_modules/tsx/dist/cli.mjs', file, ...args, '--list'], {
        encoding: 'utf8',
      }),
    ) as Command[];
    listed.set(key, commands);
  }
  return commands;
};
const testsIn = (commands: Command[]) =>
  commands.flatMap(({ args }) => args.filter((arg) => arg.endsWith('.test.ts')));

test('every test file is run by exactly one check of full validation and none is orphaned', () => {
  const npmPlan = listing('tooling/testing/test.ts');
  const apiPlan = listing('tooling/ci/checks.ts', ['api']);
  const dashboardPlan = listing('tooling/ci/checks.ts', ['checks']);
  const files = names('tests', /\.test\.ts$/).filter((file) => !file.startsWith('tests/browser/'));
  assert(files.length > 0);
  const native = Object.values(nativeShards).flat();
  const scheduled = [...testsIn(apiPlan), ...testsIn(dashboardPlan), ...native];
  assert.deepEqual(scheduled.sort(), files);
  assert.equal(new Set(scheduled).size, files.length);
  assert.deepEqual(testsIn(apiPlan), coreTests());
  assert.deepEqual(testsIn(dashboardPlan), dashboardTests);
  assert.deepEqual([...dashboardTests].sort(), names('tests/dashboard', /\.test\.ts$/));
  for (const file of dashboardTests) assert(fs.existsSync(file), `${file} does not exist`);
  // npm test and CI share recursive discovery; support files are never executable tests.
  assert.deepEqual(allTests(), files);
  assert.deepEqual(testsIn(npmPlan), files);
  assert.deepEqual(
    files.filter((file) => !/^tests\/(api|dashboard|providers|tooling)\//.test(file)),
    [],
  );
  const scripts = JSON.parse(fs.readFileSync('package.json', 'utf8')).scripts;
  assert.equal(scripts.test, 'tsx tooling/testing/test.ts');

  // Native suites run only in their real-browser shard, without skip-only API launches.
  assert.equal(new Set(native).size, native.length);
  for (const file of native) {
    assert(files.includes(file), `${file} is not a test file`);
    assert(!testsIn(apiPlan).includes(file), `${file} is also scheduled by the API check`);
  }
  const gated = files.filter((file) =>
    readsEnvironment(fs.readFileSync(file, 'utf8'), file, 'DISPATCH_TEST_NATIVE'),
  );
  assert.deepEqual(gated, [...native].sort(), 'a native suite is missing from its shard list');
  assert(native.includes(nativeRealTimeout.file));
  assert(nativeRealTimeout.title.length > 0);
  const timeoutSource = fs.readFileSync(nativeRealTimeout.file, 'utf8');
  assert(readsEnvironment(timeoutSource, nativeRealTimeout.file, 'DISPATCH_TEST_REAL_TIMEOUTS'));
  assert(timeoutSource.includes(nativeRealTimeout.title), 'the timeout sentinel title must exist');
  const timeoutWorkflow = fs.readFileSync('.github/workflows/native-timeouts.yml', 'utf8');
  assert.match(workflowField(timeoutWorkflow, 'on', 'schedule').body, /\bcron:/);
  workflowField(timeoutWorkflow, 'on', 'workflow_dispatch');
  const timeoutSteps = workflowField(timeoutWorkflow, 'jobs', 'native-timeout', 'steps').body;
  assert.match(timeoutSteps, /ref:\s*\$\{\{\s*inputs\.ref\s*\|\|\s*github\.sha\s*\}\}/);
  assert.match(timeoutSteps, /run:\s*npm run test:browseros\s+--\s+--real-timeouts\s*$/m);

  // Read Playwright's real config, not its quote or formatting choices.
  assert.equal(path.resolve(playwrightConfig.testDir!), path.resolve('tests/browser'));
  assert.equal(playwrightConfig.testMatch, undefined);
  assert.equal(playwrightConfig.testIgnore, undefined);
  assert.deepEqual(
    names('tests', /\.spec\.ts$/).filter((file) => !file.startsWith('tests/browser/')),
    [],
  );
  assert(names('tests/browser', /\.spec\.ts$/).length > 0);
  assert.deepEqual(names('tests/browser', /\.test\.ts$/), []);

  // Python source checks and real compiler/host checks cover every module once in CI.
  const python = names('tests', /\.py$/);
  assert.deepEqual(
    python.filter((file) => !/^tests\/tooling\/[a-z_]+_test\.py$/.test(file)),
    [],
    'a Python file in tests/ is not matched by the unittest discovery pattern',
  );
  const pythonCommands = apiPlan.filter(
    ({ command, args }) => command === 'python3' && args.includes('unittest'),
  );
  assert.deepEqual(pythonCommands, [pythonTests('rules'), pythonTests('integration')]);
  const pythonScheduled = pythonCommands.flatMap(({ args }) =>
    args.slice(2).map((module) => `tests/tooling/${module}.py`),
  );
  assert.deepEqual(pythonScheduled.sort(), python);
  assert.equal(new Set(pythonScheduled).size, python.length);
  assert.deepEqual(allPythonTests(), python);
  assert.equal(new Set(pythonIntegrationTests).size, pythonIntegrationTests.length);
  assert(pythonIntegrationTests.length > 0);
  for (const file of pythonIntegrationTests)
    assert(python.includes(file), `${file} does not exist`);
  assert.deepEqual(
    pythonRuleTests(),
    python.filter((file) => !pythonIntegrationTests.includes(file)),
  );
  const operator = listing('tooling/ci/checks.ts', ['core']).find(
    ({ name }) => name === 'operator probe compilation',
  );
  assert(operator, 'operator probes must compile in CI without executing');
  assert.deepEqual(operator.args, [
    'check',
    '--locked',
    '-p',
    'dispatch-backend',
    '--tests',
    '--features',
    'operator-probes',
  ]);
  for (const directory of ['tooling', 'dashboard/src', 'shared', 'services'])
    assert.deepEqual(names(directory, /\.(test|spec)\.tsx?$|_test\.py$/), []);
});

test('check:rules retains dashboard and source-rule tests, and source lints run locally and in CI', () => {
  const apiPlan = listing('tooling/ci/checks.ts', ['api']);
  const dashboardPlan = listing('tooling/ci/checks.ts', ['checks']);
  const rulesPlan = listing('tooling/ci/rules.ts');
  assert.deepEqual(
    rulesPlan.filter(({ command, args }) => command === 'python3' && args.includes('unittest')),
    [pythonTests('rules')],
  );
  assert.equal(new Set(ruleTests).size, ruleTests.length);
  assert.deepEqual(testsIn(rulesPlan), ruleTests);
  for (const file of dashboardTests) assert(ruleTests.includes(file), `${file} is not a rule`);
  for (const file of ruleTests.filter((file) => !dashboardTests.includes(file)))
    assert(testsIn(apiPlan).includes(file), `${file} is not run by the API check`);
  assert(sourceLints.length > 0);
  assert.equal(new Set(sourceLints).size, sourceLints.length);
  for (const file of sourceLints) {
    assert(fs.existsSync(file), `${file} does not exist`);
    for (const plan of [rulesPlan, dashboardPlan])
      assert.equal(
        plan.filter(({ args }) => args.includes(file)).length,
        1,
        `${file} must run exactly once`,
      );
  }
  const scripts = JSON.parse(fs.readFileSync('package.json', 'utf8')).scripts;
  assert.equal(scripts['check:rules'], 'tsx tooling/ci/rules.ts');
});

test('npm test forwards filter options as arguments while retaining recursive discovery', () => {
  const commands = listing('tooling/testing/test.ts', ['--test-name-pattern=some test']);
  assert.equal(commands.length, 1);
  assert(commands[0]!.args.includes('--test-name-pattern=some test'));
  assert.equal(
    commands[0]!.args.filter((arg) => arg === '--test-name-pattern=some test').length,
    1,
  );
  assert.deepEqual(testsIn(commands), allTests());
  assert(!commands[0]!.args.includes('--list'));
});

test('pr:prepare watches only sources and tests that exist', () => {
  assert(watchedTests.length > 0);
  for (const { sources, tests } of watchedTests) {
    assert(sources.length > 0 && tests.length > 0);
    for (const file of sources) assert(fs.existsSync(file), `${file} does not exist`);
    for (const file of tests) {
      assert(fs.existsSync(file), `${file} does not exist`);
      assert.match(file, /^tests\/.+\.(test|spec)\.ts$/);
    }
  }
});
