import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import type { Command } from '../ci/execution-plan.js';

// `npm run test:feature <name>`, `test:collector <site>`, `test:core <part>` and `test:mcp`: one
// owner's tests of every kind, found by the folder they sit in, and run as CI runs them. Each command is printed
// before it runs; `--list` prints them and runs nothing, `--no-build` skips the builds. `--only`
// keeps one kind of them, `--file` one test file; `--debug` packages the browser tests' runtime
// with the debug backend, as `dispatchdev test --owner` runs them.
//
//   Rust      its crate's tests, `cargo test -p <crate>`. A core part's are the core crate's,
//             filtered to the part; one file's, its module's.
//   node      tests/api/ and tests/frontend/, and tests/native/ suites no native shard holds,
//             the API and native ones against the debug backend.
//   browser   tests/browser/ through `npm run test:ui`, against the built artifact.
//   native    the shards of test-plan.json that hold its tests/native/ suites.

const usage = `Usage: npm run test:feature -- <name> [options]
       npm run test:collector -- <site> [options]
       npm run test:core -- <part> [options]
       npm run test:mcp -- [options]

  --only <kind>    only its rust, api, frontend, browser or native tests
  --file <path>    only one of its test files
  --debug          package the browser tests' runtime with the debug backend
  --list           print the commands and run nothing
  --no-build       skip the builds`;
const homes = { feature: 'features', collector: 'collectors', core: 'core', mcp: 'mcp' } as const;
export type Kind = keyof typeof homes;
/** `at` as the file system resolves it, through any link, or as given when it doesn't exist. */
const real = (at: string) => {
  try {
    return fs.realpathSync(at);
  } catch {
    return at;
  }
};
/** The kinds of an owner's tests, each by the folder of its tests/ they sit in. */
const LAYERS = {
  rust: 'backend',
  api: 'api',
  frontend: 'frontend',
  browser: 'browser',
  native: 'native',
};
export type Layer = keyof typeof LAYERS;
/** Packages the browser tests' runtime with the debug backend, as `dispatchdev test` does. */
const DEBUG_PACKAGE = ['run', 'build', '--', '--debug'];

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

/** The file a crate's `[lib]` is, by its path from the root. */
function libraryOf(root: string, crate: string) {
  const lib = /^\[lib\][^[]*?^path\s*=\s*"([^"]+)"/m.exec(read(root, `${crate}/Cargo.toml`))?.[1];
  return path.posix.join(crate, lib ?? 'src/lib.rs');
}
/** A `mod` a `#[path]` mounts from where it says, past any other attribute. */
const MOUNT = /#\[path = "([^"]+)"\](?:\s*#\[[^\]]*\])*\s*(?:pub(?:\([^)]*\))?\s+)?mod (\w+);/g;
/**
 * The module `file` is, and the file declaring it: by a `#[path]` in any of the crate's
 * `sources`, or by `mod <name>;` in the file of the folder holding it, or of the folder it is.
 */
function declaring(sources: Map<string, string>, roots: string[], file: string) {
  for (const [source, text] of sources)
    for (const [, at, module] of text.matchAll(MOUNT))
      if (path.posix.join(path.posix.dirname(source), at!) === file)
        return { by: source, module: module! };
  const folder = path.posix.basename(file) === 'mod.rs';
  const module = path.posix.basename(folder ? path.posix.dirname(file) : file, '.rs');
  const home = path.posix.dirname(folder ? path.posix.dirname(file) : file);
  const declared = new RegExp(
    `^(?<!#\\[path[^\\n]*\\n)\\s*(pub(\\([^)]*\\))?\\s+)?mod ${module};`,
    'm',
  );
  const by = [
    `${home}/mod.rs`,
    `${home}.rs`,
    ...roots.filter((each) => path.posix.dirname(each) === home),
  ].find((candidate) => candidate !== file && declared.test(sources.get(candidate) ?? ''));
  return by ? { by, module } : undefined;
}
/**
 * The Rust tests of one file of a crate: the test program it is or is a module of, or the
 * library's, filtered to the path of the module it is.
 */
function rustFile(root: string, crate: string, file: string, notes: string[]): Command {
  const name = packageOf(root, `${crate}/Cargo.toml`)!;
  const programs = testTargets(root, `${crate}/Cargo.toml`);
  const library = libraryOf(root, crate);
  const sources = new Map(
    files(root, crate, /\.rs$/).map((source) => [source, read(root, source)]),
  );
  const roots = [library, ...programs.map((program) => program.file)];
  const modules: string[] = [];
  const seen = new Set<string>();
  for (let at: string | undefined = file; at && !seen.has(at);) {
    seen.add(at);
    const program = programs.find((target) => target.file === at);
    if (program || at === library) {
      const filter = modules.length ? ['--', `${modules.join('::')}::`] : [];
      return program
        ? cargo('Rust tests', ['-p', name, '--test', program.name, ...filter])
        : cargo('Rust module tests', ['-p', name, '--lib', ...filter]);
    }
    const found = declaring(sources, roots, at);
    if (found) modules.unshift(found.module);
    at = found?.by;
  }
  notes.push(`No module of ${name} is ${file}, as the runner reads them: all its tests run.`);
  return cargo('Rust tests', ['-p', name]);
}

/** The Rust tests: the owner's crate, or the core crate's part; only `file`'s if it names one. */
function rust(
  root: string,
  kind: Kind,
  dir: string,
  part: string,
  notes: string[],
  file?: string,
): Command[] {
  const own = `${dir}/Cargo.toml`;
  if (file) {
    const crate = exists(root, own) ? dir : kind === 'core' ? 'core' : undefined;
    return crate && exists(root, `${crate}/Cargo.toml`) ? [rustFile(root, crate, file, notes)] : [];
  }
  if (exists(root, own)) return [cargo('Rust tests', ['-p', packageOf(root, own)!])];
  if (kind === 'core' && exists(root, 'core/Cargo.toml')) {
    const crate = packageOf(root, 'core/Cargo.toml')!;
    const targets = testTargets(root, 'core/Cargo.toml');
    // The part's integration test files: a [[test]] program of their own, or modules of core's
    // one `integration` program, named for their files.
    const own: string[] = [];
    const modules: string[] = [];
    for (const file of files(root, `${dir}/tests/backend/integration`, /\.rs$/)) {
      if (path.posix.dirname(file) !== `${dir}/tests/backend/integration`) continue;
      const target = targets.find((target) => target.file === file);
      if (target?.name === 'integration') continue;
      if (target) own.push(target.name);
      else modules.push(`${path.posix.basename(file, '.rs')}::`);
    }
    notes.push(`The core crate's tests whose path names ${part}:: are this part's module tests.`);
    return [
      cargo('Rust module tests', ['-p', crate, '--lib', '--', `${part}::`]),
      ...(own.length
        ? [
            cargo('Rust integration tests', [
              '-p',
              crate,
              ...own.flatMap((name) => ['--test', name]),
            ]),
          ]
        : []),
      ...(modules.length
        ? [
            cargo('Rust integration tests', [
              '-p',
              crate,
              '--test',
              'integration',
              '--',
              ...modules,
            ]),
          ]
        : []),
    ];
  }
  return [];
}

export type Options = {
  /** Whether the builds the tests need run before them. */
  build?: boolean;
  /** The browser tests' runtime packaged with the debug backend, rather than the release one. */
  debug?: boolean;
  /** Only the tests of one kind. */
  only?: Layer;
  /** Only one test file, by its path from the root. */
  file?: string;
};

/** The kind of test `file` is, by the folder of the owner's tests/ it sits in. */
function layerOf(root: string, dir: string, file: string) {
  const folder = file.startsWith(`${dir}/tests/`) ? file.split('/')[dir.split('/').length + 1] : '';
  const layer = (Object.keys(LAYERS) as Layer[]).find((each) => LAYERS[each] === folder);
  const test = layer === 'rust' ? /\.rs$/ : layer === 'browser' ? /\.spec\.ts$/ : /\.test\.ts$/;
  if (!layer || !test.test(file) || !exists(root, file))
    throw new Error(`--file names one of ${dir}'s tests, under ${dir}/tests/: ${file} is none`);
  return layer;
}

export function ownerTests(
  root: string,
  kind: Kind,
  name: string,
  { build = true, debug = false, only, file }: Options = {},
) {
  // The MCP is one owner in one folder, with no name of its own.
  const dir = kind === 'mcp' ? homes.mcp : `${homes[kind]}/${name}`;
  if ((kind !== 'mcp' && !/^[a-z][a-z0-9_]*$/.test(name)) || !exists(root, dir)) {
    const known = fs
      .readdirSync(path.join(root, homes[kind]), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
    throw new Error(`No ${kind} named ${name}: ${known.join(', ')}`);
  }
  if (only && file) throw new Error('Choose --only or --file.');
  const layers = Object.keys(LAYERS);
  if (only && !layers.includes(only))
    throw new Error(`--only takes ${layers.slice(0, -1).join(', ')} or ${layers.at(-1)}.`);
  const picked =
    file &&
    path.posix.normalize(path.isAbsolute(file) ? path.relative(real(root), real(file)) : file);
  const layer = picked ? layerOf(root, dir, picked) : only;
  /** The owner's tests of a kind, as many as it runs. */
  const tests = (each: Layer, pattern: RegExp) =>
    !layer || layer === each
      ? files(root, `${dir}/tests/${LAYERS[each]}`, pattern).filter(
          (test) => !picked || test === picked,
        )
      : [];
  const notes: string[] = [];
  const commands = !layer || layer === 'rust' ? rust(root, kind, dir, name, notes, picked) : [];

  const plan = JSON.parse(read(root, 'tooling/ci/test-plan.json')) as {
    native: Record<string, string[]>;
  };
  const native = tests('native', /\.test\.ts$/);
  const sharded = new Set(Object.values(plan.native).flat());
  const api = tests('api', /\.test\.ts$/);
  const unsharded = native.filter((test) => !sharded.has(test));
  const node = [...api, ...tests('frontend', /\.test\.ts$/), ...unsharded];
  if (node.length) {
    // The API and native tests run against the debug backend; the frontend's need none.
    if (build && (api.length || unsharded.length))
      commands.push({
        name: 'debug build',
        command: 'tooling/cli/dispatchdev',
        args: ['build'],
      });
    commands.push({
      name: 'node tests',
      command: 'node',
      args: ['node_modules/tsx/dist/cli.mjs', '--test', ...node],
    });
  }

  const browser = tests('browser', /\.spec\.ts$/);
  if (browser.length) {
    if (build)
      commands.push({
        name: 'build',
        command: 'npm',
        args: debug ? DEBUG_PACKAGE : ['run', 'build'],
      });
    commands.push({
      name: 'browser tests',
      command: 'npm',
      args: ['run', 'test:ui', '--', ...browser],
    });
  }

  for (const [shard, suites] of Object.entries(plan.native)) {
    if (!suites.some((test) => native.includes(test))) continue;
    const others = suites.filter((test) =>
      picked ? test !== picked : !test.startsWith(`${dir}/`),
    );
    if (others.length) notes.push(`The ${shard} shard also runs ${others.join(', ')}.`);
    commands.push({
      name: `native shard ${shard}`,
      command: 'npm',
      args: ['run', 'test:browseros', '--', '--shard', shard],
    });
  }
  if (!commands.length) notes.push(`${dir} has no ${only ? `${only} ` : ''}tests yet.`);
  return { commands, notes };
}

const quote = (arg: string) =>
  /^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replace(/'/g, `'\\''`)}'`;
export const shell = ({ command, args }: Command) => [command, ...args].map(quote).join(' ');

/** The line's flags and its options' values, or undefined for what it doesn't take. */
function parse(rest: string[]) {
  const flags = new Set<string>();
  const options = new Map<string, string>();
  for (let index = 0; index < rest.length; index++) {
    const [flag, inline] = rest[index]!.split(/=(.*)/s, 2) as [string, string | undefined];
    if (['--list', '--no-build', '--debug'].includes(flag) && inline === undefined) flags.add(flag);
    else if (['--only', '--file'].includes(flag)) {
      const value = inline ?? rest[++index];
      if (!value || value.startsWith('--')) throw new Error(`${flag} needs a value.`);
      options.set(flag, value);
    } else return undefined;
  }
  return { flags, options };
}

function main(argv: string[]) {
  const [kind, ...given] = argv;
  // The MCP's tests take no name.
  const [name, ...rest] = kind === 'mcp' ? ['', ...given] : given;
  const root = path.resolve(import.meta.dirname, '../..');
  let plan: ReturnType<typeof ownerTests>;
  let line: ReturnType<typeof parse>;
  try {
    line = parse(rest);
    if (!kind || !(kind in homes) || (kind !== 'mcp' && (!name || name.startsWith('-'))) || !line) {
      process.stderr.write(`${usage}\n`);
      return 1;
    }
    plan = ownerTests(root, kind as Kind, name ?? '', {
      build: !line.flags.has('--no-build'),
      debug: line.flags.has('--debug'),
      only: line.options.get('--only') as Layer | undefined,
      file: line.options.get('--file'),
    });
  } catch (error) {
    process.stderr.write(`${(error as Error).message}\n`);
    return 1;
  }
  for (const note of plan.notes) process.stdout.write(`# ${note}\n`);
  if (line.flags.has('--list')) {
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
