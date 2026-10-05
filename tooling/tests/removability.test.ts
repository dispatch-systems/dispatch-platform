import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {
  appManifest,
  commandLine,
  featureMap,
  leftOut,
  plan,
  reached,
  readFeatures,
  repositoryRoot as root,
  typesConfig,
  withoutFeatures,
  type FeatureMap,
} from '../ci/removability.js';
import { workflowField } from '../testing/workflow.js';

// The removability proof's plan, as a dry run computes it: which features each removal leaves
// out, and the commands it runs. Nothing here builds.

const map = (entries: Record<string, string[]>): FeatureMap =>
  Object.fromEntries(Object.entries(entries).map(([name, uses]) => [name, { depends_on: uses }]));

test('a feature is left out with every feature that declares it, however indirectly', () => {
  const features = map({ a: [], b: ['a'], c: ['b', 'paycom'], d: [], e: ['d'] });
  assert.deepEqual(leftOut(features, 'a'), ['a', 'b', 'c']);
  assert.deepEqual(leftOut(features, 'b'), ['b', 'c']);
  assert.deepEqual(leftOut(features, 'c'), ['c']);
  assert.deepEqual(leftOut(features, 'e'), ['e']);
  // A collector it uses is never left out: collectors stay in every build.
  assert.throws(() => leftOut(features, 'paycom'), /paycom is not a feature/);
});

test('the build keeps every other feature, and each step runs with that set', () => {
  const features = map({ uniforms: [], timecard: ['driver_match'], driver_match: [], home: [] });
  const removal = plan(features, 'driver_match', '/tmp/bundle');
  assert.deepEqual(removal.leftOut, ['driver_match', 'timecard']);
  assert.deepEqual(removal.kept, ['home', 'uniforms']);
  assert.deepEqual(removal.aside, ['features/driver_match', 'features/timecard']);
  const cargo = '-p dispatch-backend --no-default-features --features home,uniforms';
  assert.deepEqual(
    removal.steps.map((step) => [step.name, commandLine(step)]),
    [
      ['build', `cargo build ${cargo}`],
      ['TypeScript', `DISPATCH_UPDATE_CONTRACTS=1 cargo test ${cargo} --lib export::`],
      ['tests', `cargo test ${cargo} --no-fail-fast`],
      ['types', `npx tsc --noEmit -p ${typesConfig}`],
      ['bundle', 'npx vite build --outDir /tmp/bundle --emptyOutDir --logLevel warn'],
    ],
  );
});

test("the feature map names the app's Cargo features, each with what it declares", () => {
  const features = readFeatures(root);
  const cargo = fs.readFileSync(path.join(root, 'app/backend/Cargo.toml'), 'utf8');
  const list = /^default = \[([^\]]*)\]/m.exec(cargo)?.[1] ?? '';
  const defaults = [...list.matchAll(/"([^"]+)"/g)].map((match) => match[1]!);
  assert.deepEqual([...defaults].sort(), Object.keys(features).sort());
  for (const [name, { depends_on }] of Object.entries(features)) {
    const enables = new RegExp(`^${name} = \\[([^\\]]*)\\]`, 'm').exec(cargo)?.[1];
    assert(enables !== undefined, `the app has no ${name} feature`);
    const declared = depends_on.filter((used) => used in features);
    for (const used of declared)
      assert(enables.includes(`"${used}"`), `the app's ${name} feature does not enable ${used}`);
  }
  // A leaf goes alone; Timecard declares Driver Match, so it goes with it.
  assert.deepEqual(leftOut(features, 'uniforms'), ['uniforms']);
  assert.deepEqual(leftOut(features, 'driver_match'), ['driver_match', 'timecard']);
});

test("the app's manifest loses the left-out features' crates, and no list names them", () => {
  const manifest = [
    '[features]',
    '# Every feature.',
    'default = [',
    '    "uniforms",',
    '    "driver_match",',
    '    "timecard",',
    ']',
    'uniforms = ["dep:dispatch-uniforms"]',
    'driver_match = ["dep:dispatch-driver-match"]',
    'timecard = ["dep:dispatch-timecard", "driver_match"]',
    'reports = ["uniforms", "timecard", "dispatch-timecard/ts", "dispatch-driver-match?/ts"]',
    'probes = ["dispatch-cortex/probes"]',
    '',
    '[dependencies]',
    'dispatch-core = { path = "../../core" }',
    'dispatch-driver-match = { path = "../../features/driver_match", optional = true }',
    'dispatch-timecard = { path = "../../features/timecard", optional = true }',
    'dispatch-uniforms = { path = "../../features/uniforms", optional = true }',
    '',
    '[dev-dependencies]',
    'dispatch-timecard = { path = "../../features/timecard", features = ["ts"] }',
    'dispatch-uniforms = { path = "../../features/uniforms", features = ["ts"] }',
    '',
    '[[test]]',
    'name = "meals"',
    'path = "../../app/tests/backend/integration/meals.rs"',
    'required-features = ["timecard"]',
    '',
  ].join('\n');
  assert.equal(
    withoutFeatures(manifest, ['driver_match', 'timecard']),
    [
      '[features]',
      '# Every feature.',
      'default = [',
      '    "uniforms",',
      ']',
      'uniforms = ["dep:dispatch-uniforms"]',
      'driver_match = []',
      'timecard = []',
      'reports = ["uniforms"]',
      'probes = ["dispatch-cortex/probes"]',
      '',
      '[dependencies]',
      'dispatch-core = { path = "../../core" }',
      'dispatch-uniforms = { path = "../../features/uniforms", optional = true }',
      '',
      '[dev-dependencies]',
      'dispatch-uniforms = { path = "../../features/uniforms", features = ["ts"] }',
      '',
      '[[test]]',
      'name = "meals"',
      'path = "../../app/tests/backend/integration/meals.rs"',
      'required-features = ["timecard"]',
      '',
    ].join('\n'),
  );
  assert.equal(withoutFeatures(manifest, []), manifest);
});

test("each removal leaves the app's manifest naming none of what it leaves out", () => {
  const features = readFeatures(root);
  const manifest = fs.readFileSync(path.join(root, appManifest), 'utf8');
  for (const feature of Object.keys(features)) {
    const out = leftOut(features, feature);
    const stripped = withoutFeatures(manifest, out);
    for (const name of out) {
      assert(!stripped.includes(`features/${name}"`), `${feature}: features/${name}`);
      assert.match(stripped, new RegExp(`^${name} = \\[\\]$`, 'm'), feature);
      assert(!new RegExp(`^\\s*"${name}",?$`, 'm').test(stripped), `${feature}: "${name}"`);
    }
    for (const name of Object.keys(features).filter((each) => !out.includes(each)))
      assert(stripped.includes(`features/${name}"`), `${feature} keeps ${name}`);
  }
});

test('a dry run prints the plan and changes nothing', () => {
  const before = fs.readFileSync(path.join(root, featureMap), 'utf8');
  const printed = execFileSync(
    process.execPath,
    [
      'node_modules/tsx/dist/cli.mjs',
      'tooling/ci/removability.ts',
      '--feature',
      'driver_match',
      '--dry-run',
    ],
    { cwd: root, encoding: 'utf8' },
  );
  const lines = printed.trim().split('\n');
  assert.match(lines[0]!, /^Leaving out driver_match, timecard; keeping /);
  assert.equal(
    lines.filter((line) =>
      /^\(moves features\/driver_match, features\/timecard aside\)$/.test(line),
    ).length,
    1,
  );
  assert.deepEqual(
    lines
      .filter((line) => !line.startsWith('('))
      .slice(1)
      .map((line) => line.split(':')[0]),
    ['build', 'TypeScript', 'tests', 'types', 'bundle'],
  );
  assert.equal(fs.readFileSync(path.join(root, featureMap), 'utf8'), before);
  assert(fs.existsSync(path.join(root, 'features/driver_match')));
});

test('a failure names the files and tests that reached into what was left out', () => {
  const output = [
    'error[E0432]: unresolved import `dispatch_uniforms`',
    '  --> app/backend/routes.rs:4:5',
    '---- catalog::the_catalog_keeps_its_pages_tabs_and_order stdout ----',
    "core/shell/frontend/shell/Nav.tsx(3,10): error TS2307: Cannot find module '../../../features/uniforms/frontend/feature.js'.",
    'Could not resolve "../../features/uniforms/frontend/feature.js" from "app/frontend/routes.tsx"',
    '  --> app/backend/routes.rs:9:1',
  ].join('\n');
  assert.deepEqual(reached(output), [
    'app/backend/routes.rs',
    'catalog::the_catalog_keeps_its_pages_tabs_and_order',
    'core/shell/frontend/shell/Nav.tsx',
    'app/frontend/routes.tsx',
  ]);
});

test('the weekly workflow runs the script once per feature of the map, the same way', () => {
  const workflow = fs.readFileSync(path.join(root, '.github/workflows/removability.yml'), 'utf8');
  assert.match(workflowField(workflow, 'on').body, /^\s*schedule:/m);
  assert.match(workflowField(workflow, 'on').body, /^\s*workflow_dispatch:/m);
  const list = workflowField(workflow, 'jobs', 'features').body;
  assert(list.includes(featureMap), 'the matrix is read from the feature map');
  const remove = workflowField(workflow, 'jobs', 'remove').body;
  assert.match(workflowField(remove, 'strategy').body, /fail-fast: false/);
  assert.match(
    remove,
    /matrix:\s*\n\s*feature: \$\{\{ fromJSON\(needs\.features\.outputs\.features\) \}\}/,
  );
  assert.match(remove, /npm run check:removability -- --feature "\$FEATURE"/);
  const scripts = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).scripts;
  assert.equal(scripts['check:removability'], 'tsx tooling/ci/removability.ts');
});
