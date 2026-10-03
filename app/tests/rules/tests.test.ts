import test from 'node:test';
import { holds, pendingNames } from './support/pending.js';
import { files, isDirectory, ownerOf, owners, templated } from './support/repo.js';
import { lexFile, rust, testAttributes } from './support/rust.js';

// plans/restructure/structure.md, "Tests": the folder decides how a test runs.
const kinds = ['api', 'browser', 'frontend', 'backend', 'native', 'mcp', 'support'];
/**
 * Where the tooling's and the hosts' own tests live, beside the owners' tests/<kind>/: their
 * tests/ folders, and those of their Rust crates, which keep Cargo's layout.
 */
const toolTests = [
  'tooling/tests',
  'ops/tests',
  ...rust()
    .crates.filter((crate) => !ownerOf(crate.manifest))
    .map((crate) => `${crate.dir}/tests`),
];
/** The owner's tests/ folder a file sits in, with its kind and the rest of its path. */
function placed(file: string) {
  const owner = ownerOf(file);
  if (!owner || !file.startsWith(`${owner.dir}/tests/`)) return undefined;
  const [kind, ...rest] = file.slice(owner.dir.length + '/tests/'.length).split('/');
  return { owner, kind: kind!, rest: rest.join('/') };
}

test("test code sits in an owner's tests/<kind>/, the tooling's or the hosts' tests/", () => {
  const stray: string[] = [];
  for (const file of files.map(templated)) {
    const at = placed(file);
    const tool = toolTests.find((directory) => file.startsWith(`${directory}/`));
    const testFile = /\.(test|spec)\.tsx?$|_test\.py$/.test(file);
    if (/(^|\/)tests\//.test(file) && !at && !tool) {
      stray.push(`${file.slice(0, file.indexOf('tests/') + 5)} is no owner's tests/`);
      continue;
    }
    if (tool) continue;
    if (testFile && !at) {
      stray.push(`${file} is a test outside tests/`);
      continue;
    }
    if (!at) continue;
    const allowed = at.owner.layer === 'app' ? [...kinds, 'rules'] : kinds;
    if (!allowed.includes(at.kind) || !at.rest) {
      stray.push(`${file} is in no tests/<kind>/`);
      continue;
    }
    // Each kind runs one way: Playwright specs, Node tests, or Rust.
    if (file.endsWith('.spec.ts') && at.kind !== 'browser')
      stray.push(`${file} is a browser spec outside tests/browser/`);
    if (file.endsWith('.test.ts') && !['api', 'frontend', 'native', 'rules'].includes(at.kind))
      stray.push(`${file} is a Node test outside tests/api/, frontend/ or native/`);
    if (file.endsWith('.rs') && !['backend', 'support'].includes(at.kind))
      stray.push(`${file} is Rust outside tests/backend/ and tests/support/`);
  }
  holds('tests', 'placement', stray);
});

// Module tests live in tests/backend/ and are mounted into their module, so they still reach
// its private code. The tooling's and the hosts' crates keep their own layout.
test('no #[test] sits outside tests/', () => {
  const inline = files
    .filter((file) => file.endsWith('.rs') && ownerOf(file) && !/(^|\/)tests\//.test(file))
    .filter((file) => testAttributes(lexFile(file)).length)
    .map((file) => `${file} has a #[test]`);
  holds('tests', 'inline tests', inline);
});

test('a module test file is mounted by exactly one module of its owner, for tests only', () => {
  const wrong: string[] = [];
  for (const file of files) {
    const at = placed(file);
    if (!at || at.kind !== 'backend' || !file.endsWith('.rs') || at.rest.startsWith('integration/'))
      continue;
    const mounts = rust().mounted.get(file) ?? [];
    if (mounts.length !== 1) wrong.push(`${file} is mounted by ${mounts.length} modules`);
    for (const mount of mounts) {
      if (ownerOf(mount.file)?.dir !== at.owner.dir)
        wrong.push(`${file} is mounted by ${mount.file}, outside its owner`);
      if (!mount.test) wrong.push(`${file} is mounted outside #[cfg(test)]`);
    }
  }
  holds('tests', 'module tests', wrong);
});

// An integration test is a [[test]] of the crate whose tests/backend/integration/ holds it:
// a feature's or collector's own, core's, or the app's.
test("each integration test is a test of its owner's crate", () => {
  const crateOf = (dir: string) =>
    dir === 'app'
      ? 'app/backend/Cargo.toml'
      : dir.startsWith('core/')
        ? 'core/Cargo.toml'
        : `${dir}/Cargo.toml`;
  const targets = rust().crates.flatMap((crate) =>
    crate.targets
      .filter((target) => target.kind === 'test')
      .map((target) => ({ manifest: crate.manifest, file: target.file })),
  );
  const wrong = new Set<string>();
  for (const file of files) {
    const at = placed(file);
    if (!at || at.kind !== 'backend' || !/^integration\/[^/]+\.rs$/.test(at.rest)) continue;
    const listed = targets.filter((target) => target.file === file);
    if (listed.length !== 1) wrong.add(`${file} is a [[test]] of ${listed.length} crates`);
    for (const { manifest } of listed)
      if (manifest !== crateOf(at.owner.dir))
        wrong.add(`${at.owner.dir}'s integration tests are tests of ${manifest}`);
  }
  holds('tests', 'integration tests', wrong);
});

test('an owner with an api/ has tests/api/', () => {
  const untested = owners()
    .filter(({ dir }) => isDirectory(`${dir}/api`) && !isDirectory(`${dir}/tests/api`))
    .map(({ dir }) => `${dir} has api/ and no tests/api/`);
  holds('tests', 'api tests', untested);
});

test('pending.json names only these checks', () => {
  pendingNames('tests', [
    'placement',
    'inline tests',
    'module tests',
    'integration tests',
    'api tests',
  ]);
});
