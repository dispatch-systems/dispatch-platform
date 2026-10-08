import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// The first step of `npm run contracts:generate`: lists every feature
// in features/ in the app, as only the app may. Its crate goes in the app's Cargo manifest, as
// a Cargo feature the product has by default and a build may leave out, and its manifest in
// the registry's list, `app/backend/features.rs`. So adding a feature is adding its folder, and
// removing one is removing it. The rules check holds the app to what this writes.

export const appManifest = 'app/backend/Cargo.toml';
export const featureList = 'app/backend/features.rs';
/** Each of the app manifest's sections that lists the features starts at this line. */
export const marker =
  '# Every feature: written from features/ by `npm run contracts:generate`, to the blank line.';

/** What the app needs of a feature's crate. */
export type Crate = {
  /** Its folder's name, which is the app's Cargo feature for it. */
  name: string;
  /** Its crate: `dispatch-driver-match`. */
  crate: string;
  /** Whether it writes its API types to TypeScript, through a `ts` feature. */
  ts: boolean;
  /** The other features whose crates it uses. */
  uses: string[];
};

const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');
/** A Cargo manifest's `[section]`, without its header. */
function section(manifest: string, name: string) {
  const lines = manifest.split('\n');
  const start = lines.findIndex((line) => line.trim() === `[${name}]`);
  if (start < 0) return '';
  const end = lines.findIndex((line, index) => index > start && /^\[/.test(line));
  return lines.slice(start + 1, end < 0 ? undefined : end).join('\n');
}

/** Every feature in `root`'s features/, by name: a folder with a manifest and a crate. */
export function crates(root: string): Crate[] {
  const dir = path.join(root, 'features');
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) =>
      ['feature.rs', 'Cargo.toml'].every((file) => fs.existsSync(path.join(dir, name, file))),
    )
    .sort()
    .map((name) => crateOf(name, read(root, `features/${name}/Cargo.toml`)));
}
/** What the app needs of the feature `name`, from its Cargo manifest. */
export function crateOf(name: string, manifest: string): Crate {
  const crate = /^name\s*=\s*"([^"]+)"/m.exec(section(manifest, 'package'))?.[1];
  if (!crate) throw new Error(`features/${name}/Cargo.toml names no package`);
  const uses = [...section(manifest, 'dependencies').matchAll(/path\s*=\s*"\.\.\/([a-z0-9_]+)"/g)]
    .map((found) => found[1]!)
    .sort();
  return { name, crate, ts: /^ts\s*=/m.test(section(manifest, 'features')), uses };
}

/** The app manifest with each section's feature list as `features` has it. */
export function wiredManifest(manifest: string, features: Crate[]) {
  const lists: Record<string, string[]> = {
    features: [
      'default = [',
      ...features.map(({ name }) => `    "${name}",`),
      ']',
      ...features.map(
        ({ name, crate, uses }) =>
          `${name} = [${[`dep:${crate}`, ...uses].map((entry) => `"${entry}"`).join(', ')}]`,
      ),
    ],
    dependencies: features.map(
      ({ name, crate }) => `${crate} = { path = "../../features/${name}", optional = true }`,
    ),
    'dev-dependencies': features
      .filter(({ ts }) => ts)
      .map(
        ({ name, crate }) => `${crate} = { path = "../../features/${name}", features = ["ts"] }`,
      ),
  };
  const lines = manifest.split('\n');
  const out: string[] = [];
  let current = '';
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index]!;
    current = /^\[([^\]]+)\]$/.exec(line)?.[1] ?? current;
    out.push(line);
    if (line !== marker) continue;
    const list = lists[current];
    if (!list) throw new Error(`${appManifest}: [${current}] lists no features`);
    out.push(...list);
    delete lists[current];
    while (index + 1 < lines.length && lines[index + 1]!.trim()) index++;
  }
  const missing = Object.keys(lists);
  if (missing.length)
    throw new Error(`${appManifest}: [${missing.join('], [')}] has no line reading ${marker}`);
  return out.join('\n');
}

/** The registry's list of features, each while the app's Cargo feature of its name is on. */
export function wiredList(features: Crate[]) {
  const ident = (crate: string) => crate.replaceAll('-', '_');
  const entries = features.map(
    ({ name, crate }) => `    #[cfg(feature = "${name}")]\n    &${ident(crate)}::FEATURE,\n`,
  );
  // Each feature that writes its API types to TypeScript lists them itself.
  const typescript = features
    .filter(({ ts }) => ts)
    .map(
      ({ name, crate }) =>
        `    #[cfg(feature = "${name}")]\n    all.extend(${ident(crate)}::typescript(cfg));\n`,
    );
  return (
    "//! Every feature in features/, written by `npm run contracts:generate`: the registry's,\n" +
    "//! each while the app's Cargo feature of its name is on. The registry puts them in their\n" +
    '//! places.\n' +
    'use dispatch_core::manifest::Feature;\n\n' +
    `pub const FEATURES: &[&Feature] = &[\n${entries.join('')}];\n\n` +
    "/// Every feature's API types in TypeScript, as each lists them, for the app's export test.\n" +
    '#[cfg(test)]\n' +
    'pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {\n' +
    '    #[allow(unused_mut)]\n' +
    '    let mut all = dispatch_core::Typescript::new();\n' +
    typescript.join('') +
    '    all\n' +
    '}\n'
  );
}

/** Each file the wiring writes, as it should read for `root`'s features. */
export function wiring(root: string) {
  const features = crates(root);
  return new Map([
    [appManifest, wiredManifest(read(root, appManifest), features)],
    [featureList, wiredList(features)],
  ]);
}

/**
 * Writes the wiring into `root`; Cargo.lock then lists the workspace's crates as they now are,
 * without the network, since builds run with `--locked`. Answers the files it changed.
 */
export function wire(root: string) {
  const changed: string[] = [];
  for (const [file, text] of wiring(root)) {
    const at = path.join(root, file);
    if (fs.existsSync(at) && fs.readFileSync(at, 'utf8') === text) continue;
    fs.writeFileSync(at, text);
    changed.push(file);
  }
  if (changed.includes(appManifest)) {
    const lock = spawnSync('cargo', ['update', '--workspace', '--offline', '--quiet'], {
      cwd: root,
      stdio: 'inherit',
    });
    if (lock.status !== 0) throw new Error('cargo update --workspace --offline failed');
  }
  return changed;
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const changed = wire(root);
  process.stdout.write(
    changed.length ? `Listed the features anew in ${changed.join(' and ')}.\n` : '',
  );
}
