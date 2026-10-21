import test from 'node:test';
import ts from 'typescript';
import { holds } from './support/holds.js';
import {
  files,
  isDirectory,
  isFile,
  isTestFile,
  ownerOf,
  owners,
  read,
  templated,
} from './support/repo.js';
import { lexFile, lineOf, rust, testAttributes } from './support/rust.js';

// The folder decides how a test runs.
const kinds = ['api', 'browser', 'frontend', 'backend', 'native', 'support'];
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
/**
 * The owner's tests/ folder a file sits in, its own or one of its integrations'
 * (`integrations/<name>/tests/`), with its kind and the rest of its path.
 */
function placed(file: string) {
  const owner = ownerOf(file);
  const tests = owner && new RegExp(`^${owner.dir}/(integrations/[^/]+/)?tests/`).exec(file);
  if (!owner || !tests) return undefined;
  const [kind, ...rest] = file.slice(tests[0].length).split('/');
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

// An integration test is a [[test]] of the crate whose tests/backend/integration/ holds it, or a
// module of exactly one, as a crate's tests share one program: a feature's or collector's own,
// core's, or the app's.
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
  const programs = new Map(targets.map(({ manifest, file }) => [file, manifest]));
  const wrong = new Set<string>();
  for (const file of files) {
    const at = placed(file);
    if (!at || at.kind !== 'backend' || !/^integration\/[^/]+\.rs$/.test(at.rest)) continue;
    const crates = [
      ...targets.filter((target) => target.file === file).map(({ manifest }) => manifest),
      ...(rust().mounted.get(file) ?? [])
        .filter((mount) => programs.has(mount.file))
        .map((mount) => programs.get(mount.file)!),
    ];
    if (crates.length !== 1)
      wrong.add(`${file} is a [[test]], or a module of one, of ${crates.length} crates`);
    for (const manifest of crates)
      if (manifest !== crateOf(at.owner.dir))
        wrong.add(`${at.owner.dir}'s integration tests are tests of ${manifest}`);
  }
  holds('tests', 'integration tests', wrong);
});

test('an owner with endpoints has tests/api/', () => {
  const untested = owners()
    .filter(({ dir }) => isFile(`${dir}/api/routes.rs`) || isDirectory(`${dir}/api/routes`))
    .filter(({ dir }) => !isDirectory(`${dir}/tests/api`))
    .map(({ dir }) => `${dir} has api/ and no tests/api/`);
  holds('tests', 'api tests', untested);
});

// A test runs whole and waits on what it waits for: an answer, an event, or a clock it moves
// itself. Nothing focuses, skips or retries one, and nothing gives one longer than the suite
// does. Only a test that skips itself unless a run opts in, as the native and artifact tests do
// for their real browsers and builds, sets a timeout of its own.
const registers = new Set(['test', 'it', 'describe', 'suite']);
const narrows = new Set(['only', 'skip', 'todo', 'fixme', 'fail']);
const lengthens = new Set(['slow', 'setTimeout']);

function waits(file: string): string[] {
  const source = ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true);
  const at = (node: ts.Node) =>
    `${file}:${source.getLineAndCharacterOfPosition(node.getStart()).line + 1}`;
  // What a file's constants hold, so `test(name, native, …)` and `{ skip: native }` read them.
  const constants = new Map<string, ts.Expression>();
  for (const statement of source.statements)
    if (ts.isVariableStatement(statement))
      for (const { name, initializer } of statement.declarationList.declarations)
        if (ts.isIdentifier(name) && initializer) constants.set(name.text, initializer);
  const named = (node: ts.Expression): ts.Expression =>
    ts.isIdentifier(node) && constants.has(node.text) ? named(constants.get(node.text)!) : node;
  // A run opts in through the environment, as `DISPATCH_TEST_NATIVE=1` does.
  const optsIn = (node: ts.Node): boolean =>
    ((ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) &&
      node.expression.getText() === 'process.env') ||
    (ts.isIdentifier(node) && constants.has(node.text) && optsIn(constants.get(node.text)!)) ||
    (ts.forEachChild(node, optsIn) ?? false);
  const options = (node: ts.Expression | undefined) => {
    const object = node && named(node);
    if (!object || !ts.isObjectLiteralExpression(object)) return new Map<string, ts.Node>();
    return new Map(
      object.properties.flatMap((property) =>
        ts.isPropertyAssignment(property) && ts.isIdentifier(property.name)
          ? [[property.name.text, property.initializer] as const]
          : ts.isShorthandPropertyAssignment(property)
            ? [[property.name.text, property.name] as const]
            : [],
      ),
    );
  };
  // The name a call is made through: `test` in `test.describe.only(…)`.
  const root = (node: ts.Expression): string | undefined =>
    ts.isIdentifier(node)
      ? node.text
      : ts.isPropertyAccessExpression(node) || ts.isCallExpression(node)
        ? root(node.expression)
        : undefined;

  const found: string[] = [];
  const visit = (node: ts.Node) => {
    if (ts.isCallExpression(node)) {
      const callee = node.expression;
      const member = ts.isPropertyAccessExpression(callee) ? callee.name.text : undefined;
      if (member === 'waitForTimeout')
        found.push(`${at(node)} waits for time to pass: wait for what it waits on`);
      else if (registers.has(root(callee) ?? '')) {
        if (member && narrows.has(member))
          found.push(`${at(node)} narrows the run with .${member}: every test runs whole`);
        if (member && lengthens.has(member))
          found.push(`${at(node)} gives a test longer with .${member}: the suite's timeout holds`);
        const given = options(member === 'configure' ? node.arguments[0] : node.arguments[1]);
        for (const option of ['only', 'todo', 'retries'])
          if (given.has(option))
            found.push(`${at(node)} sets ${option}: every test runs whole, once`);
        const skip = given.get('skip');
        const optedIn = skip !== undefined && optsIn(skip);
        if (skip && !optedIn) found.push(`${at(node)} skips a test no run opts in to`);
        if (given.has('timeout') && !optedIn)
          found.push(`${at(node)} sets a timeout, which only a test a run opts in to may`);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

test('tests run whole and wait on what they wait for, not on time passing', () => {
  holds(
    'tests',
    'waits',
    files
      .filter((file) => /\.tsx?$/.test(templated(file)) && isTestFile(templated(file)))
      .flatMap(waits),
  );
});

test('an ignored Rust test says why', () => {
  const silent = files
    .filter((file) => templated(file).endsWith('.rs'))
    .flatMap((file) => {
      const { text, masked } = lexFile(file);
      return [...masked.matchAll(/#\[\s*ignore\s*\]/g)].map(
        (match) => `${file}:${lineOf(text, match.index)} ignores a test: #[ignore = "<why>"]`,
      );
    });
  holds('tests', 'ignored tests', silent);
});
