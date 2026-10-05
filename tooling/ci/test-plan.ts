import fs from 'node:fs';

// Which check runs which `<owner>/tests/<kind>/*.test.ts` file. `test-plan.json` is the only list;
// `browseros-check.py` reads `native` from it too.
const plan = JSON.parse(fs.readFileSync(new URL('./test-plan.json', import.meta.url), 'utf8')) as {
  dashboard: string[];
  rules: string[];
  lint: string[];
  native: Record<string, string[]>;
  nativeRealTimeout: { file: string; title: string };
  pythonIntegration: string[];
  watch: { sources: string[]; tests: string[] }[];
};

/** Dashboard logic: the checks job runs these against the packaged build. */
export const dashboardTests = plan.dashboard;
/**
 * Source-wide rules that need no build: `npm run check:rules` runs them before a push. They
 * also run in CI, the dashboard ones in the checks job and the others in the api job.
 */
export const ruleTests = [...plan.dashboard, ...plan.rules];
/** Source policies run as lints by local rules and the CI checks job. */
export const sourceLints = plan.lint;
/** Native collector shards, run with a real browser by `npm run test:browseros`. */
export const nativeShards = plan.native;
export const nativeTests = Object.values(nativeShards).flat();
/** One real wall-clock timeout regression, run on its dedicated scheduled/manual lane. */
export const nativeRealTimeout = plan.nativeRealTimeout;
/** Real compiler/installed-manager checks; source-only rules do not build Rust. */
export const pythonIntegrationTests = plan.pythonIntegration;
/** Where the Python tests live: the tooling's own and the host scripts'. */
export const pythonTestDirs = ['tooling/tests', 'ops/tests'];
export function allPythonTests() {
  return pythonTestDirs
    .flatMap((directory) =>
      fs
        .readdirSync(directory)
        .filter((name) => name.endsWith('_test.py'))
        .map((name) => `${directory}/${name}`),
    )
    .sort();
}
export function pythonRuleTests() {
  return allPythonTests().filter((file) => !pythonIntegrationTests.includes(file));
}
/**
 * Sources whose changes break tests elsewhere, which `dispatchdev check` names with the
 * tests the diff changes. Each group comes from queue runs such changes failed.
 */
export const watchedTests = plan.watch;
/** Where tests live: each owner's `tests/<kind>/`, the tooling's `tooling/tests/` and ops' `ops/tests/`. */
export const testRoots = ['app', 'collectors', 'core', 'features', 'tooling', 'ops'];
/** Every other test file: the api job runs these, so nothing runs twice in a full run. */
export function allTests(roots = testRoots) {
  return roots
    .flatMap((root) =>
      fs.readdirSync(root, { recursive: true, encoding: 'utf8' }).map((name) => `${root}/${name}`),
    )
    .filter((file) => file.endsWith('.test.ts') && /(^|\/)tests\//.test(file))
    .sort();
}

export function coreTests() {
  return allTests().filter((file) => !dashboardTests.includes(file) && !nativeTests.includes(file));
}
