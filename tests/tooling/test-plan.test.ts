import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import playwrightConfig from '../../playwright.config.js';
import {
  allTests,
  coreTests,
  dashboardTests,
  nativeShards,
  ruleTests,
  sourceLints,
  watchedTests,
} from '../../tooling/ci/test-plan.js';
import type { Command } from '../../tooling/ci/execution-plan.js';
import { readsEnvironment } from '../../tooling/testing/source-analysis.js';

const names = (directory: string, pattern: RegExp) =>
  fs
    .readdirSync(directory, { recursive: true, encoding: 'utf8' })
    .filter((name) => pattern.test(name))
    .map((name) => `${directory}/${name}`)
    .sort();

// Run the entry points, stopping before execution: these are their actual child commands.
const listing = (file: string, args: string[] = []): Command[] =>
  JSON.parse(
    execFileSync(process.execPath, ['node_modules/tsx/dist/cli.mjs', file, ...args, '--list'], {
      encoding: 'utf8',
    }),
  );
const npmPlan = listing('tooling/testing/test.ts');
const apiPlan = listing('tooling/ci/checks.ts', ['api']);
const dashboardPlan = listing('tooling/ci/checks.ts', ['checks']);
const rulesPlan = listing('tooling/ci/rules.ts');
const testsIn = (commands: Command[]) =>
  commands.flatMap(({ args }) => args.filter((arg) => arg.endsWith('.test.ts')));

test('every test file is run by exactly one check of full validation and none is orphaned', () => {
  const files = names('tests', /\.test\.ts$/).filter((file) => !file.startsWith('tests/browser/'));
  assert(files.length > 20);
  const scheduled = [...testsIn(apiPlan), ...testsIn(dashboardPlan)];
  assert.deepEqual(scheduled.sort(), files);
  assert.equal(new Set(scheduled).size, files.length);
  assert.deepEqual(testsIn(apiPlan), coreTests());
  assert.deepEqual(testsIn(dashboardPlan), dashboardTests);
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

  // Native suites run in the API job, where their environment gate skips real-browser work.
  const native = Object.values(nativeShards).flat();
  assert.equal(new Set(native).size, native.length);
  for (const file of native) assert(testsIn(apiPlan).includes(file), `${file} is not a test file`);
  const gated = files.filter((file) =>
    readsEnvironment(fs.readFileSync(file, 'utf8'), file, 'DISPATCH_TEST_NATIVE'),
  );
  assert.deepEqual(gated, [...native].sort(), 'a native suite is missing from its shard list');

  // Read Playwright's real config, not its quote or formatting choices.
  assert.equal(path.resolve(playwrightConfig.testDir!), path.resolve('tests/browser'));
  assert.equal(playwrightConfig.testMatch, undefined);
  assert.equal(playwrightConfig.testIgnore, undefined);
  assert.deepEqual(
    names('tests', /\.spec\.ts$/).filter((file) => !file.startsWith('tests/browser/')),
    [],
  );
  assert(names('tests/browser', /\.spec\.ts$/).length > 10);
  assert.deepEqual(names('tests/browser', /\.test\.ts$/), []);

  // Python discovery owns tests/tooling and the exact suffix used by the actual runners.
  const python = names('tests', /\.py$/);
  assert.deepEqual(
    python.filter((file) => !/^tests\/tooling\/[a-z_]+_test\.py$/.test(file)),
    [],
    'a Python file in tests/ is not matched by the unittest discovery pattern',
  );
  for (const plan of [apiPlan, rulesPlan]) {
    const command = plan.find(
      ({ command, args }) => command === 'python3' && args.includes('unittest'),
    );
    assert(command, 'Python tooling tests must be scheduled');
    assert.deepEqual(command.args, [
      '-m',
      'unittest',
      'discover',
      '-s',
      'tests/tooling',
      '-p',
      '*_test.py',
    ]);
  }
  for (const directory of ['tooling', 'dashboard/src', 'shared', 'services'])
    assert.deepEqual(names(directory, /\.(test|spec)\.tsx?$|_test\.py$/), []);
});

test('check:rules retains dashboard and source-rule tests, and source lints run locally and in CI', () => {
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
