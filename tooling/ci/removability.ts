import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

// The proof that each feature can be removed (plans/restructure/enforcement.md, section 4).
// It leaves one feature out, with every feature that declares it, and the rest of the
// product must still build, pass the app's tests, write its TypeScript, and typecheck and
// bundle its frontend. .github/workflows/removability.yml runs it weekly for every feature;
// locally it runs the same way:
//
//   npm run check:removability -- --feature uniforms [--dry-run]
//
// Every step runs with the left-out features really gone: their directories are moved aside
// and the app's manifest no longer names their crates, so nothing, product or test, can still
// use them. The Cargo steps run without `--locked`, since the lock loses their crates. The
// typecheck covers all but the app's cross-owner tests and the tooling, which name every
// feature as the app's Rust tests do. Afterwards the checkout is as it was: the directories,
// the manifest, the lock and the generated TypeScript all come back.

const usage = 'Usage: npm run check:removability -- --feature <name> [--dry-run]';

/** The feature map's part this reads: what each feature declares it uses. */
export type FeatureMap = Record<string, { depends_on: string[] }>;
export type Step = {
  name: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
  /** Whether the steps after it need it to pass: the build, and the TypeScript it writes. */
  needed?: boolean;
};
export type Plan = { leftOut: string[]; kept: string[]; aside: string[]; steps: Step[] };

export const repositoryRoot = path.resolve(import.meta.dirname, '../..');
export const featureMap = 'app/generated/features.json';
/** The app's manifest, which a run rewrites without the left-out features' crates. */
export const appManifest = 'app/backend/Cargo.toml';
/** The workspace's lock, which the Cargo steps rewrite without their crates. */
const lock = 'Cargo.lock';
/** The frontend's program without the app's cross-owner tests and the tooling. */
export const typesConfig = 'tooling/ci/removability.tsconfig.json';
/**
 * Where a run keeps what it moves aside: outside `.build`, which a build replaces whole, so
 * what a run killed before it could restore leaves stays until it is put back.
 */
const workspace = '.removability';

export function readFeatures(root: string): FeatureMap {
  return JSON.parse(fs.readFileSync(path.join(root, featureMap), 'utf8')) as FeatureMap;
}

/** `feature` and every feature that declares one of those, however indirectly, by name. */
export function leftOut(features: FeatureMap, feature: string): string[] {
  if (!Object.hasOwn(features, feature))
    throw new Error(
      `${feature} is not a feature; the features are ${Object.keys(features).join(', ')}`,
    );
  const out = new Set([feature]);
  for (let grew = true; grew;) {
    grew = false;
    for (const [name, { depends_on }] of Object.entries(features))
      if (!out.has(name) && depends_on.some((used) => out.has(used))) {
        out.add(name);
        grew = true;
      }
  }
  return Object.keys(features)
    .filter((name) => out.has(name))
    .sort();
}

/**
 * The app's manifest without the features `out`: their dependency and dev-dependency lines go,
 * and no feature list names them or their crates. Their own features stay declared, empty, so
 * the `cfg`s and `required-features` that name them read as off rather than unknown.
 */
export function withoutFeatures(manifest: string, out: readonly string[]): string {
  // A feature's crate, by the directory its path names.
  const dependency = /^([\w-]+)\s*=\s*\{.*\bpath\s*=\s*"(?:[^"]*\/)?features\/([\w-]+)"/;
  const crates = new Set<string>();
  for (const line of manifest.split('\n')) {
    const found = dependency.exec(line);
    if (found && out.includes(found[2]!)) crates.add(found[1]!);
  }
  // `name`, `dep:crate`, `crate/feature` and `crate?/feature` name a left-out feature.
  const named = (entry: string) =>
    out.includes(entry) || crates.has(entry.replace(/^dep:/, '').split(/\??\//)[0]!);
  const unnamed = (line: string) =>
    line
      .replace(/"([^"]*)"\s*,?\s*/g, (quoted, entry: string) => (named(entry) ? '' : quoted))
      .replace(/,\s*\]/, ']');
  const lines: string[] = [];
  let section = '';
  // In a feature's list that goes on past its first line, and whether that feature is left out.
  let open: { out: boolean } | undefined;
  for (const line of manifest.split('\n')) {
    const header = /^\[{1,2}([^\]]+)\]{1,2}\s*$/.exec(line);
    if (header) section = header[1]!;
    if (section.endsWith('dependencies') && crates.has(dependency.exec(line)?.[1] ?? '')) continue;
    if (section !== 'features' || header) {
      lines.push(line);
      continue;
    }
    const definition = open ? undefined : /^([\w-]+)\s*=\s*\[/.exec(line);
    const list = definition ? { out: out.includes(definition[1]!) } : open;
    open = list && !line.includes(']') ? list : undefined;
    if (!list) lines.push(line);
    else if (list.out) {
      if (definition) lines.push(`${definition[1]} = []`);
    } else {
      const left = unnamed(line);
      // A line of a list that named only what was left out goes with it.
      if (left.trim() || !line.trim()) lines.push(left);
    }
  }
  return lines.join('\n');
}

/** What leaving `feature` out runs, in order. `bundle` is where the frontend is built to. */
export function plan(features: FeatureMap, feature: string, bundle: string): Plan {
  const out = leftOut(features, feature);
  const kept = Object.keys(features)
    .filter((name) => !out.includes(name))
    .sort();
  // Unlocked: the lock loses the left-out crates.
  const app = ['-p', 'dispatch-backend', '--no-default-features'];
  const cargo = [...app, '--features', kept.join(',')];
  return {
    leftOut: out,
    kept,
    aside: out.map((name) => `features/${name}`),
    steps: [
      { name: 'build', command: 'cargo', args: ['build', ...cargo], needed: true },
      {
        name: 'TypeScript',
        command: 'cargo',
        args: ['test', ...cargo, '--lib', 'export::'],
        env: { DISPATCH_UPDATE_CONTRACTS: '1' },
        needed: true,
      },
      { name: 'tests', command: 'cargo', args: ['test', ...cargo, '--no-fail-fast'] },
      { name: 'types', command: 'npx', args: ['tsc', '--noEmit', '-p', typesConfig] },
      {
        name: 'bundle',
        command: 'npx',
        args: ['vite', 'build', '--outDir', bundle, '--emptyOutDir', '--logLevel', 'warn'],
      },
    ],
  };
}

/** How a step reads in a log: its environment, command and arguments. */
export const commandLine = ({ command, args, env }: Step) =>
  [...Object.entries(env ?? {}).map(([key, value]) => `${key}=${value}`), command, ...args].join(
    ' ',
  );

/**
 * What a failed step's output says broke: the files a compiler or the bundler names, and the
 * tests that failed, each once, in the order they are first named.
 */
export function reached(output: string): string[] {
  const found = new Set<string>();
  const patterns = [
    // rustc: `  --> app/backend/routes.rs:4:5`
    /^\s*--> ([^\s:]+):\d+:\d+/gm,
    // a failed Rust test: `---- catalog::the_catalog_keeps_its_pages stdout ----`
    /^---- (\S+) stdout ----$/gm,
    // tsc: `core/shell/frontend/x.ts(3,10): error TS2307: …`
    /^([^\s(]+)\(\d+,\d+\): error TS\d+/gm,
    // Vite: `Could not resolve "../../features/x/y.js" from "core/z.ts"`
    /Could not resolve "[^"]+" from "([^"]+)"/g,
  ];
  for (const pattern of patterns)
    for (const match of output.matchAll(pattern)) found.add(match[1]!);
  return [...found];
}

async function run(step: Step, root: string): Promise<{ ok: boolean; output: string }> {
  const child = spawn(step.command, step.args, {
    cwd: root,
    env: { ...process.env, ...step.env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  child.stdout.on('data', (chunk: Buffer) => {
    output += chunk;
    process.stdout.write(chunk);
  });
  child.stderr.on('data', (chunk: Buffer) => {
    output += chunk;
    process.stderr.write(chunk);
  });
  const ok = await new Promise<boolean>((resolve) => {
    child.once('error', () => resolve(false));
    child.once('close', (code) => resolve(code === 0));
  });
  return { ok, output };
}

/** Every file the app's export writes: each owner's api/generated/, the map and the list. */
function generated(root: string): string[] {
  const owners = ['core', 'collectors', 'features'].flatMap((top) =>
    fs.readdirSync(path.join(root, top)).map((name) => `${top}/${name}/api/generated`),
  );
  return ['app/frontend/features.ts', 'app/generated', ...owners].filter((file) =>
    fs.existsSync(path.join(root, file)),
  );
}

/**
 * Leaves the features out of the checkout, keeping what that and the steps change, and puts it
 * all back as it was. It refuses to start over what an interrupted run left, which `restore`
 * would lose.
 */
function checkout(root: string, removal: Plan) {
  const store = path.join(root, workspace);
  if (fs.existsSync(store))
    throw new Error(
      `${workspace} holds what an earlier run moved aside: put its features/ back and delete it`,
    );
  const before = [...generated(root), appManifest, lock];
  for (const file of before)
    fs.cpSync(path.join(root, file), path.join(store, 'kept', file), { recursive: true });
  const moved: string[] = [];
  return {
    leaveOut() {
      for (const dir of removal.aside) {
        fs.mkdirSync(path.dirname(path.join(store, dir)), { recursive: true });
        fs.renameSync(path.join(root, dir), path.join(store, dir));
        moved.push(dir);
      }
      const manifest = path.join(root, appManifest);
      fs.writeFileSync(
        manifest,
        withoutFeatures(fs.readFileSync(manifest, 'utf8'), removal.leftOut),
      );
    },
    restore() {
      for (const dir of moved.splice(0)) fs.renameSync(path.join(store, dir), path.join(root, dir));
      for (const file of new Set([...generated(root), ...before]))
        fs.rmSync(path.join(root, file), { recursive: true, force: true });
      for (const file of before)
        fs.cpSync(path.join(store, 'kept', file), path.join(root, file), { recursive: true });
      fs.rmSync(store, { recursive: true, force: true });
    },
  };
}

const seconds = (since: number) => `${Math.round((Date.now() - since) / 1000)}s`;

export async function main(argv: string[], root = repositoryRoot) {
  const at = argv.indexOf('--feature');
  const feature = at >= 0 ? argv[at + 1] : undefined;
  if (
    !feature ||
    argv.some((arg, index) => arg.startsWith('--') && index !== at && arg !== '--dry-run')
  )
    throw new Error(usage);
  const removal = plan(readFeatures(root), feature, path.join(root, workspace, 'dashboard'));
  const named = removal.leftOut.join(', ');
  process.stdout.write(`Leaving out ${named}; keeping ${removal.kept.join(', ')}.\n`);
  if (argv.includes('--dry-run')) {
    process.stdout.write(`(moves ${removal.aside.join(', ')} aside)\n`);
    process.stdout.write(`(drops their crates from ${appManifest})\n`);
    for (const step of removal.steps) process.stdout.write(`${step.name}: ${commandLine(step)}\n`);
    return true;
  }
  const started = Date.now();
  const state = checkout(root, removal);
  const interrupted = () => {
    state.restore();
    process.exit(130);
  };
  process.once('SIGINT', interrupted);
  process.once('SIGTERM', interrupted);
  const reports: string[] = [];
  try {
    state.leaveOut();
    for (const step of removal.steps) {
      const since = Date.now();
      process.stdout.write(`[${step.name}] ${commandLine(step)}\n`);
      const { ok, output } = await run(step, root);
      process.stdout.write(`[${step.name}] ${ok ? 'passed' : 'failed'} in ${seconds(since)}\n`);
      if (ok) continue;
      const names = reached(output);
      reports.push(
        `Leaving out ${named} broke ${step.name}` +
          (names.length ? `, at:\n${names.map((name) => `  ${name}`).join('\n')}` : '.'),
      );
      if (step.needed) break;
    }
  } finally {
    state.restore();
    process.off('SIGINT', interrupted);
    process.off('SIGTERM', interrupted);
  }
  if (reports.length) {
    const report = `${reports.join('\n')}\n`;
    process.stderr.write(report);
    if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, report);
    return false;
  }
  process.stdout.write(
    `Without ${named}, the rest builds, passes and bundles (${seconds(started)}).\n`,
  );
  return true;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename)) {
  try {
    process.exitCode = (await main(process.argv.slice(2))) ? 0 : 1;
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
