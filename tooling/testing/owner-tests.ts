import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import type { Command } from '../ci/execution-plan.js';

// `npm run test:feature <name>`, `test:collector <site>` and `test:core <part>`: one owner's tests
// of every kind, found by the folder they sit in, and run as CI runs them. Each command is printed
// before it runs; `--list` prints them and runs nothing, `--no-build` skips the builds.
//
//   Rust      its crate's tests, `cargo test -p <crate>`. A core part's are the core crate's,
//             filtered to the part.
//   node      tests/api/ and tests/frontend/, and tests/native/ suites no native shard holds,
//             against the debug backend.
//   browser   tests/browser/ through `npm run test:ui`, against the built artifact.
//   native    the shards of test-plan.json that hold its tests/native/ suites.

const usage = `Usage: npm run test:feature -- <name> [--list] [--no-build]
       npm run test:collector -- <site> [--list] [--no-build]
       npm run test:core -- <part> [--list] [--no-build]`;
const homes = { feature: 'features', collector: 'collectors', core: 'core' } as const;
export type Kind = keyof typeof homes;

const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');
const exists = (root: string, file: string) => fs.existsSync(path.join(root, file));
function files(root: string, dir: string, pattern: RegExp) {
  if (!exists(root, dir)) return [];
  return fs
    .readdirSync(path.join(root, dir), { recursive: true, encoding: 'utf8' })
    .map((name) => path.posix.join(dir, name.split(path.sep).join('/')))
    .filter((file) => pattern.test(file))
    .sort();
}

/** A crate's package name. */
function packageOf(root: string, manifest: string) {
  return /^\[package\][^[]*?^name\s*=\s*"([^"]+)"/m.exec(read(root, manifest))?.[1];
}
/** A crate's `[[test]]` targets, each with its path from the root. */
function testTargets(root: string, manifest: string) {
  return read(root, manifest)
    .split(/^\[\[test\]\]\s*$/m)
    .slice(1)
    .map((block) => block.split(/^\[/m)[0]!)
    .map((block) => ({
      name: /^name\s*=\s*"([^"]+)"/m.exec(block)?.[1] ?? '',
      file: path.posix.normalize(
        path.posix.join(
          path.posix.dirname(manifest),
          /^path\s*=\s*"([^"]+)"/m.exec(block)?.[1] ?? '',
        ),
      ),
    }));
}
const cargo = (name: string, args: string[]): Command => ({
  name,
  command: 'cargo',
  args: ['test', '--locked', ...args],
});

/** The Rust tests: the owner's crate, or the core crate's part. */
function rust(root: string, kind: Kind, dir: string, part: string, notes: string[]): Command[] {
  const own = `${dir}/Cargo.toml`;
  if (exists(root, own)) return [cargo('Rust tests', ['-p', packageOf(root, own)!])];
  if (kind === 'core' && exists(root, 'core/Cargo.toml')) {
    const crate = packageOf(root, 'core/Cargo.toml')!;
    const targets = testTargets(root, 'core/Cargo.toml').filter(({ file }) =>
      file.startsWith(`${dir}/`),
    );
    notes.push(`The core crate's tests whose path names ${part}:: are this part's module tests.`);
    return [
      cargo('Rust module tests', ['-p', crate, '--lib', '--', `${part}::`]),
      ...(targets.length
        ? [
            cargo('Rust integration tests', [
              '-p',
              crate,
              ...targets.flatMap(({ name }) => ['--test', name]),
            ]),
          ]
        : []),
    ];
  }
  return [];
}

export function ownerTests(root: string, kind: Kind, name: string, options = { build: true }) {
  const dir = `${homes[kind]}/${name}`;
  if (!/^[a-z][a-z0-9_]*$/.test(name) || !exists(root, dir)) {
    const known = fs
      .readdirSync(path.join(root, homes[kind]), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
    throw new Error(`No ${kind} named ${name}: ${known.join(', ')}`);
  }
  const notes: string[] = [];
  const commands = rust(root, kind, dir, name, notes);

  const plan = JSON.parse(read(root, 'tooling/ci/test-plan.json')) as {
    native: Record<string, string[]>;
  };
  const native = files(root, `${dir}/tests/native`, /\.test\.ts$/);
  const sharded = new Set(Object.values(plan.native).flat());
  const node = [
    ...files(root, `${dir}/tests/api`, /\.test\.ts$/),
    ...files(root, `${dir}/tests/frontend`, /\.test\.ts$/),
    ...native.filter((file) => !sharded.has(file)),
  ];
  if (node.length) {
    if (options.build)
      commands.push({
        name: 'debug build',
        command: 'python3',
        args: ['tooling/build/cargo-build.py'],
      });
    commands.push({
      name: 'node tests',
      command: 'node',
      args: ['node_modules/tsx/dist/cli.mjs', '--test', ...node],
    });
  }

  const browser = files(root, `${dir}/tests/browser`, /\.spec\.ts$/);
  if (browser.length) {
    if (options.build) commands.push({ name: 'build', command: 'npm', args: ['run', 'build'] });
    commands.push({
      name: 'browser tests',
      command: 'npm',
      args: ['run', 'test:ui', '--', ...browser],
    });
  }

  for (const [shard, suites] of Object.entries(plan.native)) {
    if (!suites.some((file) => native.includes(file))) continue;
    const others = suites.filter((file) => !file.startsWith(`${dir}/`));
    if (others.length) notes.push(`The ${shard} shard also runs ${others.join(', ')}.`);
    commands.push({
      name: `native shard ${shard}`,
      command: 'npm',
      args: ['run', 'test:browseros', '--', '--shard', shard],
    });
  }
  if (exists(root, `${dir}/tests/mcp`))
    notes.push('Its agent eval questions in tests/mcp/ run on demand, with `npm run agents:eval`.');
  if (!commands.length) notes.push(`${dir} has no tests yet.`);
  return { commands, notes };
}

const quote = (arg: string) =>
  /^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replace(/'/g, `'\\''`)}'`;
export const shell = ({ command, args }: Command) => [command, ...args].map(quote).join(' ');

function main(argv: string[]) {
  const [kind, name, ...rest] = argv;
  const unknown = rest.filter((arg) => !['--list', '--no-build'].includes(arg));
  if (!kind || !(kind in homes) || !name || name.startsWith('-') || unknown.length) {
    process.stderr.write(`${usage}\n`);
    return 1;
  }
  const root = path.resolve(import.meta.dirname, '../..');
  let plan: ReturnType<typeof ownerTests>;
  try {
    plan = ownerTests(root, kind as Kind, name, { build: !rest.includes('--no-build') });
  } catch (error) {
    process.stderr.write(`${(error as Error).message}\n`);
    return 1;
  }
  for (const note of plan.notes) process.stdout.write(`# ${note}\n`);
  if (rest.includes('--list')) {
    for (const command of plan.commands) process.stdout.write(`$ ${shell(command)}\n`);
    return 0;
  }
  for (const command of plan.commands) {
    process.stdout.write(`$ ${shell(command)}\n`);
    const result = spawnSync(command.command, command.args, { cwd: root, stdio: 'inherit' });
    if (result.error || result.status !== 0) {
      process.stderr.write(`Failed: ${command.name}\n`);
      return result.status || 1;
    }
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  process.exitCode = main(process.argv.slice(2));
