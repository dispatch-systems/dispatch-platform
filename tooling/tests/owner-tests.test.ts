import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ownerTests, shell } from '../testing/owner-tests.js';

// test:feature, test:collector and test:core, listed rather than run: the commands they would
// run name the owner's tests of every kind, found by the folder each sits in.

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
/** The app crate's integration targets whose file sits under `dir`. */
const targets = (dir: string) =>
  fs
    .readFileSync('app/backend/Cargo.toml', 'utf8')
    .split('[[test]]')
    .slice(1)
    .map((block) => ({
      name: /name = "([^"]+)"/.exec(block)![1]!,
      file: path.posix.normalize(`app/backend/${/path = "([^"]+)"/.exec(block)![1]}`),
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
  const dvic = targets('features/dvic');
  assert(dvic.includes('dvic'));
  const node = [
    ...owned('features/dvic/tests/api', /\.test\.ts$/),
    ...owned('features/dvic/tests/frontend', /\.test\.ts$/),
  ];
  assert(node.includes('features/dvic/tests/api/api-dvic.test.ts'));
  const browser = owned('features/dvic/tests/browser', /\.spec\.ts$/);
  assert(browser.includes('features/dvic/tests/browser/dvic.spec.ts'));
  assert.deepEqual(commands, [
    `cargo test --locked -p dispatch-backend ${dvic.map((name) => `--test ${name}`).join(' ')}`,
    'python3 tooling/build/cargo-build.py',
    `node node_modules/tsx/dist/cli.mjs --test ${node.join(' ')}`,
    'npm run build',
    `npm run test:ui -- ${browser.join(' ')}`,
  ]);
});

test("a collector's list runs its sharded native suites through their shards and the rest with node", () => {
  const { status, commands } = list('collector', 'cortex', '--no-build');
  assert.equal(status, 0);
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
    !commands.some((command) => /cargo-build|npm run build/.test(command)),
    '--no-build builds nothing',
  );
});

test("a core part's list names its integration tests, API tests and browser specs", () => {
  const { status, commands } = list('core', 'accounts');
  assert.equal(status, 0);
  assert.deepEqual(targets('core/accounts'), ['accounts']);
  assert.equal(commands[0], 'cargo test --locked -p dispatch-backend --test accounts');
  const api = owned('core/accounts/tests/api', /\.test\.ts$/);
  assert(api.includes('core/accounts/tests/api/api-sign-in.test.ts'));
  assert(commands.includes(`node node_modules/tsx/dist/cli.mjs --test ${api.join(' ')}`));
  assert(
    commands.includes(
      `npm run test:ui -- ${owned('core/accounts/tests/browser', /\.spec\.ts$/).join(' ')}`,
    ),
  );
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
