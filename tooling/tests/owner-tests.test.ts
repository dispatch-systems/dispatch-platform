import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ownerTests, shell } from '../testing/owner-tests.js';

// test:feature, test:collector, test:core, test:mcp and test:app, listed rather than run: the
// commands they would run name the owner's tests of every kind, found by the folder each sits in.

const root = path.resolve(import.meta.dirname, '../..');
const list = (...args: string[]) => {
  const result = spawnSync(
    process.execPath,
    ['node_modules/tsx/dist/cli.mjs', 'tooling/testing/owner-tests.ts', ...args, '--list'],
    { cwd: root, encoding: 'utf8' },
  );
  const lines = result.stdout.split('\n');
  return {
    status: result.status,
    stderr: result.stderr,
    commands: lines.filter((line) => line.startsWith('$ ')).map((line) => line.slice(2)),
    notes: lines.filter((line) => line.startsWith('# ')).map((line) => line.slice(2)),
  };
};
/** The owner's tests of one kind on disk. */
const owned = (dir: string, pattern: RegExp) =>
  fs
    .readdirSync(dir, { recursive: true, encoding: 'utf8' })
    .map((name) => `${dir}/${name}`)
    .filter((file) => pattern.test(file))
    .sort();
/** A crate's integration targets whose file sits under `dir`. */
const targets = (dir: string, manifest: string) =>
  fs
    .readFileSync(manifest, 'utf8')
    .split('[[test]]')
    .slice(1)
    .map((block) => ({
      name: /name = "([^"]+)"/.exec(block)![1]!,
      file: path.posix.normalize(
        `${path.posix.dirname(manifest)}/${/path = "([^"]+)"/.exec(block)![1]}`,
      ),
    }))
    .filter(({ file }) => file.startsWith(`${dir}/`))
    .map(({ name }) => name);
const shards = JSON.parse(fs.readFileSync('tooling/ci/test-plan.json', 'utf8')).native as Record<
  string,
  string[]
>;

test("a feature's list names its Rust, API, frontend and browser tests, each where CI runs it", () => {
  const { status, commands } = list('feature', 'dvic');
  assert.equal(status, 0);
  // Its crate runs its module tests and its integration tests, dvic among them.
  assert(targets('features/dvic', 'features/dvic/Cargo.toml').includes('dvic'));
  const node = [
    ...owned('features/dvic/tests/api', /\.test\.ts$/),
    ...owned('features/dvic/tests/frontend', /\.test\.ts$/),
  ];
  assert(node.includes('features/dvic/tests/api/api-dvic.test.ts'));
  const browser = owned('features/dvic/tests/browser', /\.spec\.ts$/);
  assert(browser.includes('features/dvic/tests/browser/dvic.spec.ts'));
  assert.deepEqual(commands, [
    'cargo test --locked -p dispatch-dvic',
    'tooling/cli/dispatchdev build',
    `node node_modules/tsx/dist/cli.mjs --test ${node.join(' ')}`,
    'npm run build',
    `npm run test:ui -- ${browser.join(' ')}`,
  ]);
});

test("a collector's list runs its sharded native suites through their shards and the rest with node", () => {
  const { status, commands } = list('collector', 'cortex', '--no-build');
  assert.equal(status, 0);
  assert.equal(commands[0], 'cargo test --locked -p dispatch-cortex');
  const native = owned('collectors/cortex/tests/native', /\.test\.ts$/);
  const sharded = Object.entries(shards).filter(([, files]) =>
    files.some((file) => native.includes(file)),
  );
  assert.deepEqual(
    sharded.map(([shard]) => shard),
    ['cortex', 'cortex-meals'],
  );
  for (const [shard] of sharded)
    assert(commands.includes(`npm run test:browseros -- --shard ${shard}`), shard);
  const unsharded = native.filter((file) => !sharded.some(([, files]) => files.includes(file)));
  assert(unsharded.includes('collectors/cortex/tests/native/cortex-discovery.test.ts'));
  assert(commands.includes(`node node_modules/tsx/dist/cli.mjs --test ${unsharded.join(' ')}`));
  assert(commands.includes('npm run test:ui -- collectors/cortex/tests/browser/cortex.spec.ts'));
  assert(
    !commands.some((command) => /dispatchdev build|npm run build/.test(command)),
    '--no-build builds nothing',
  );
});

test("a core part's list names its integration tests, API tests and browser specs", () => {
  const { status, commands } = list('core', 'accounts');
  assert.equal(status, 0);
  // Its integration tests are a module of core's one integration program.
  assert.deepEqual(targets('core/accounts', 'core/Cargo.toml'), []);
  assert.deepEqual(commands.slice(0, 2), [
    'cargo test --locked -p dispatch-core --lib -- accounts::',
    'cargo test --locked -p dispatch-core --test integration -- accounts::',
  ]);
  const api = owned('core/accounts/tests/api', /\.test\.ts$/);
  assert(api.includes('core/accounts/tests/api/api-sign-in.test.ts'));
  assert(commands.includes(`node node_modules/tsx/dist/cli.mjs --test ${api.join(' ')}`));
  assert(
    commands.includes(
      `npm run test:ui -- ${owned('core/accounts/tests/browser', /\.spec\.ts$/).join(' ')}`,
    ),
  );
});

test("the MCP's list, which takes no name, names its crate's tests, API tests and browser specs", () => {
  const { status, commands } = list('mcp');
  assert.equal(status, 0);
  assert.equal(commands[0], 'cargo test --locked -p dispatch-mcp');
  const api = owned('mcp/tests/api', /\.test\.ts$/);
  assert(commands.includes(`node node_modules/tsx/dist/cli.mjs --test ${api.join(' ')}`));
  assert(
    commands.includes(`npm run test:ui -- ${owned('mcp/tests/browser', /\.spec\.ts$/).join(' ')}`),
  );
});

test("the app's list, which takes no name, names its backend's tests and the rest of its own, and where its rules run", () => {
  const { status, commands, notes } = list('app');
  assert.equal(status, 0);
  assert.equal(commands[0], 'cargo test --locked -p dispatch-backend');
  const native = owned('app/tests/native', /\.test\.ts$/);
  const sharded = Object.entries(shards).filter(([, files]) =>
    files.some((file) => native.includes(file)),
  );
  const unsharded = native.filter((file) => !sharded.some(([, files]) => files.includes(file)));
  const node = [
    ...owned('app/tests/api', /\.test\.ts$/),
    ...owned('app/tests/frontend', /\.test\.ts$/),
    ...unsharded,
  ];
  assert(commands.includes(`node node_modules/tsx/dist/cli.mjs --test ${node.join(' ')}`));
  assert(
    commands.includes(`npm run test:ui -- ${owned('app/tests/browser', /\.spec\.ts$/).join(' ')}`),
  );
  for (const [shard] of sharded)
    assert(commands.includes(`npm run test:browseros -- --shard ${shard}`), shard);
  assert.match(notes.join('\n'), /app\/tests\/rules\/ run with npm run check:rules/);
});

test('--only keeps one kind of tests and the build it needs, --file one test, --debug the debug package', () => {
  const api = owned('features/dvic/tests/api', /\.test\.ts$/);
  assert.deepEqual(list('feature', 'dvic', '--only', 'api').commands, [
    'tooling/cli/dispatchdev build',
    `node node_modules/tsx/dist/cli.mjs --test ${api.join(' ')}`,
  ]);
  // The frontend's tests need no backend.
  assert.deepEqual(list('feature', 'dvic', '--only', 'frontend').commands, [
    `node node_modules/tsx/dist/cli.mjs --test ${owned('features/dvic/tests/frontend', /\.test\.ts$/).join(' ')}`,
  ]);
  const spec = 'features/dvic/tests/browser/dvic.spec.ts';
  assert.deepEqual(list('feature', 'dvic', '--file', spec, '--debug').commands, [
    'npm run build -- --debug',
    `npm run test:ui -- ${spec}`,
  ]);
  assert.deepEqual(list('feature', 'dvic', '--file', path.join(root, api[0]!)).commands.slice(1), [
    `node node_modules/tsx/dist/cli.mjs --test ${api[0]}`,
  ]);
  // A path through a link to the checkout, as a worktree's often is, names the same test.
  const links = fs.mkdtempSync(path.join(os.tmpdir(), 'owner-tests-'));
  try {
    fs.symlinkSync(root, path.join(links, 'checkout'));
    const linked = path.join(links, 'checkout', api[0]!);
    assert.deepEqual(list('feature', 'dvic', '--file', linked).commands.slice(1), [
      `node node_modules/tsx/dist/cli.mjs --test ${api[0]}`,
    ]);
  } finally {
    fs.rmSync(links, { recursive: true, force: true });
  }
  const only = list('feature', 'dvic', '--only', 'api,frontend');
  assert.equal(only.status, 1);
  assert.equal(only.stderr, '--only takes rust, api, frontend, browser or native.\n');
  const source = list('feature', 'dvic', '--file', 'features/dvic/feature.rs');
  assert.equal(source.status, 1);
  assert.match(source.stderr, /--file names one of features\/dvic's tests/);
  assert.equal(list('feature', 'dvic', '--only', 'api', '--file', spec).status, 1);
});

test('an owner that does not exist is named with the ones that do', () => {
  const missing = list('feature', 'nowhere');
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /No feature named nowhere: .*dvic.*uniforms/);
  const usage = list('owner', 'dvic');
  assert.equal(usage.status, 1);
  assert.match(usage.stderr, /Usage: npm run test:feature/);
});

/** A repository holding only what the runner reads. */
function repository(files: Record<string, string>) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-owner-tests-'));
  for (const [name, content] of Object.entries({
    'tooling/ci/test-plan.json': '{ "native": {} }',
    ...files,
  })) {
    fs.mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
    fs.writeFileSync(path.join(dir, name), content);
  }
  return dir;
}

test("once an owner is a crate, its Rust tests are the crate's", () => {
  const dir = repository({
    'features/parking/Cargo.toml':
      '[package]\nname = "dispatch-parking"\n\n[lib]\npath = "feature.rs"\n',
    'features/parking/tests/backend/feature.rs': '',
    'features/parking/tests/api/parking.test.ts': '',
  });
  try {
    const { commands } = ownerTests(dir, 'feature', 'parking', { build: false });
    assert.deepEqual(commands.map(shell), [
      'cargo test --locked -p dispatch-parking',
      'node node_modules/tsx/dist/cli.mjs --test features/parking/tests/api/parking.test.ts',
    ]);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('one Rust test file runs as its program, or as the module of the program or library it is', () => {
  const dir = repository({
    'features/parking/Cargo.toml':
      '[package]\nname = "dispatch-parking"\n\n[lib]\npath = "feature.rs"\n\n' +
      '[[test]]\nname = "parking"\npath = "tests/backend/integration/parking.rs"\n',
    'features/parking/feature.rs': 'mod backend;\n',
    'features/parking/backend/mod.rs': 'pub(crate) mod storage;\n',
    'features/parking/backend/storage.rs':
      '#[cfg(test)]\n#[path = "../tests/backend/storage.rs"]\nmod tests;\n',
    'features/parking/tests/backend/storage.rs': '',
    'features/parking/tests/backend/integration/parking.rs': 'mod rules;\n',
    'features/parking/tests/backend/integration/rules.rs': '',
    'features/parking/tests/backend/stray.rs': '',
  });
  try {
    const run = (file: string) =>
      ownerTests(dir, 'feature', 'parking', { file: `features/parking/tests/backend/${file}` });
    const cargo = 'cargo test --locked -p dispatch-parking';
    assert.deepEqual(run('storage.rs').commands.map(shell), [
      `${cargo} --lib -- backend::storage::tests::`,
    ]);
    assert.deepEqual(run('integration/parking.rs').commands.map(shell), [
      `${cargo} --test parking`,
    ]);
    assert.deepEqual(run('integration/rules.rs').commands.map(shell), [
      `${cargo} --test parking -- rules::`,
    ]);
    // A file no module is runs with the rest, which it says.
    const stray = run('stray.rs');
    assert.deepEqual(stray.commands.map(shell), [cargo]);
    assert.match(stray.notes.join('\n'), /stray\.rs/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("one of the app's Rust test files runs as the module it is of its backend's library or program", () => {
  const dir = repository({
    'app/backend/Cargo.toml':
      '[package]\nname = "dispatch-backend"\n\n[lib]\npath = "lib.rs"\n\n' +
      '[[test]]\nname = "integration"\npath = "../../app/tests/backend/integration/main.rs"\n',
    'app/backend/lib.rs': '#[cfg(test)]\n#[path = "../tests/backend/audit.rs"]\nmod audit_areas;\n',
    'app/tests/backend/audit.rs': '',
    'app/tests/backend/integration/main.rs': 'mod support;\n\nmod oauth;\n',
    'app/tests/backend/integration/oauth.rs': '',
  });
  try {
    const run = (file: string) =>
      ownerTests(dir, 'app', '', { file: `app/tests/backend/${file}` }).commands.map(shell);
    const cargo = 'cargo test --locked -p dispatch-backend';
    assert.deepEqual(run('audit.rs'), [`${cargo} --lib -- audit_areas::`]);
    // The program's root sits in app/tests/, outside the crate's folder.
    assert.deepEqual(run('integration/oauth.rs'), [`${cargo} --test integration -- oauth::`]);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a core part's Rust tests are the core crate's, filtered to the part", () => {
  const dir = repository({
    'core/Cargo.toml':
      '[package]\nname = "dispatch-core"\n\n[lib]\npath = "core.rs"\n\n' +
      '[[test]]\nname = "accounts"\npath = "accounts/tests/backend/integration/accounts.rs"\n\n' +
      '[[test]]\nname = "jobs"\npath = "collection/tests/backend/integration/jobs.rs"\n',
    'core/accounts/tests/backend/integration/accounts.rs': '',
  });
  try {
    const { commands, notes } = ownerTests(dir, 'core', 'accounts', { build: false });
    assert.deepEqual(commands.map(shell), [
      'cargo test --locked -p dispatch-core --lib -- accounts::',
      'cargo test --locked -p dispatch-core --test accounts',
    ]);
    assert.match(notes.join('\n'), /accounts::/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a core part's integration tests in core's one program run by their modules' names", () => {
  const dir = repository({
    'core/Cargo.toml':
      '[package]\nname = "dispatch-core"\n\n[lib]\npath = "core.rs"\n\n' +
      '[[test]]\nname = "integration"\npath = "db/tests/backend/integration/main.rs"\n',
    'core/db/tests/backend/integration/main.rs': '',
    'core/db/tests/backend/integration/storage.rs': '',
    'core/server/tests/backend/integration/browser_update.rs': '',
    'core/server/tests/backend/integration/mail.rs': '',
  });
  try {
    const run = (part: string) =>
      ownerTests(dir, 'core', part, { build: false }).commands.map(shell)[1];
    assert.equal(
      run('server'),
      'cargo test --locked -p dispatch-core --test integration -- browser_update:: mail::',
    );
    // The program's own root holds no tests of the part it sits in.
    assert.equal(run('db'), 'cargo test --locked -p dispatch-core --test integration -- storage::');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
