import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import playwrightConfig from '../../playwright.config.js';
import {
  allTests,
  testRoots,
  allPythonTests,
  coreTests,
  dashboardTests,
  nativeShards,
  nativeRealTimeout,
  pythonIntegrationTests,
  pythonRuleTests,
  pythonTestDirs,
  ruleTests,
  sourceLints,
  watchedTests,
} from '../ci/test-plan.js';
import { pythonTests, type Command } from '../ci/execution-plan.js';
import { readsEnvironment } from '../testing/source-analysis.js';
import { workflowField } from '../testing/workflow.js';

const names = (directory: string, pattern: RegExp) =>
  fs
    .readdirSync(directory, { recursive: true, encoding: 'utf8' })
    .filter((name) => pattern.test(name))
    .map((name) => `${directory}/${name}`)
    .sort();
// Test files in every owner's tests/ folder, the tooling's and ops'.
const owned = (pattern: RegExp) =>
  testRoots
    .flatMap((root) => names(root, pattern))
    .filter((file) => /(^|\/)tests\//.test(file))
    .sort();
const browserSpec = /(^|\/)tests\/browser\//;

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
  const files = owned(/\.test\.ts$/).filter((file) => !browserSpec.test(file));
  assert(files.length > 0);
  const native = Object.values(nativeShards).flat();
  const scheduled = [...testsIn(apiPlan), ...testsIn(dashboardPlan), ...native];
  assert.deepEqual(scheduled.sort(), files);
  assert.equal(new Set(scheduled).size, files.length);
  assert.deepEqual(testsIn(apiPlan), coreTests());
  assert.deepEqual(testsIn(dashboardPlan), dashboardTests);
  assert.deepEqual(
    [...dashboardTests].sort(),
    files.filter(
      (file) =>
        /\/tests\/frontend\//.test(file) ||
        /^app\/tests\/rules\/(dashboard-structure|unused-css)\.test\.ts$/.test(file),
    ),
  );
  for (const file of dashboardTests) assert(fs.existsSync(file), `${file} does not exist`);
  // npm test and CI share recursive discovery; support files are never executable tests.
  assert.deepEqual(allTests(), files);
  assert.deepEqual(testsIn(npmPlan), files);
  assert.deepEqual(
    files.filter(
      (file) =>
        !/^((app|(core|collectors|features)\/[a-z_]+)\/tests\/(api|frontend|native|rules)|(tooling|ops)\/tests)\//.test(
          file,
        ),
    ),
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
  assert.equal(path.resolve(playwrightConfig.testDir!), path.resolve('.'));
  assert.equal(playwrightConfig.testMatch, '**/tests/browser/**/*.spec.ts');
  assert.equal(playwrightConfig.testIgnore, undefined);
  assert.equal(playwrightConfig.respectGitIgnore, true);
  assert.deepEqual(
    owned(/\.spec\.ts$/).filter((file) => !browserSpec.test(file)),
    [],
  );
  assert(owned(/\.spec\.ts$/).length > 0);
  assert.deepEqual(
    owned(/\.test\.ts$/).filter((file) => browserSpec.test(file)),
    [],
  );

  // Python source checks and real compiler/host checks cover every module once in CI.
  const python = pythonTestDirs.flatMap((directory) => names(directory, /\.py$/)).sort();
  assert.deepEqual(
    python.filter((file) => !/^(tooling|ops)\/tests\/[a-z_]+_test\.py$/.test(file)),
    [],
    'a Python test file is not matched by the unittest discovery pattern',
  );
  const modules = python.map((file) => path.basename(file, '.py'));
  assert.equal(new Set(modules).size, modules.length, 'two Python test modules share a name');
  const pythonCommands = apiPlan.filter(
    ({ command, args }) => command === 'python3' && args.includes('unittest'),
  );
  assert.deepEqual(pythonCommands, [pythonTests('rules'), pythonTests('integration')]);
  const pythonScheduled = pythonCommands.flatMap(({ args }) =>
    args.slice(2).map((module) => python.find((file) => path.basename(file, '.py') === module)!),
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
  // The probes live in the collectors, whose own tests hold most of them; the app runs those
  // that measure a feature's part too. Each such crate has an operator-probes feature.
  const probing = testRoots
    .flatMap((root) => names(root, /(^|\/)Cargo\.toml$/))
    .map((manifest) => fs.readFileSync(manifest, 'utf8'))
    .filter((text) => /^\[features\]\n(?:(?!\[)[^\n]*\n)*operator-probes\s*=/m.test(text))
    .map((text) => /^name = "([^"]+)"/m.exec(text)![1]!)
    .sort();
  for (const name of ['dispatch-backend', 'dispatch-cortex', 'dispatch-paycom'])
    assert(probing.includes(name), `${name} has an operator-probes feature`);
  assert.deepEqual(operator.args, [
    'check',
    '--locked',
    ...probing.flatMap((name) => ['-p', name]),
    '--tests',
    '--features',
    'operator-probes',
  ]);
  // Tooling and ops keep their tests in their own tests/ folder; shared and services have none.
  for (const directory of ['tooling', 'ops', 'shared', 'services'])
    assert.deepEqual(
      names(directory, /\.(test|spec)\.tsx?$|_test\.py$/).filter(
        (file) =>
          !file.startsWith(`${directory}/tests/`) || !['tooling', 'ops'].includes(directory),
      ),
      [],
    );
  // Owners keep tests only in their tests/ folder, never beside the code.
  assert.deepEqual(
    testRoots
      .flatMap((root) => names(root, /\.(test|spec)\.tsx?$|_test\.py$/))
      .filter((file) => !/(^|\/)tests\//.test(file)),
    [],
  );
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
      assert.match(file, /(^|\/)tests\/.+\.(test|spec)\.ts$/);
    }
  }
});
