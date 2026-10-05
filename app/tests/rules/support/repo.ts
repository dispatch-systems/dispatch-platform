import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

// The rule tests read the repository as it is checked out, from its root, and build nothing.
export const root = path.resolve(import.meta.dirname, '../../../..');

/** Every file Git would keep, committed or not, relative to the root with `/` separators. */
export const files: readonly string[] = execFileSync(
  'git',
  ['ls-files', '-z', '--cached', '--others', '--exclude-standard'],
  { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
)
  .split('\0')
  .filter((file) => file && fs.existsSync(path.join(root, file)))
  .sort();
const fileSet = new Set(files);

const texts = new Map<string, string>();
export function read(file: string): string {
  let text = texts.get(file);
  if (text === undefined) {
    text = fs.readFileSync(path.join(root, file), 'utf8');
    texts.set(file, text);
  }
  return text;
}
export const isFile = (file: string) => fileSet.has(file);
export const exists = (file: string) =>
  fileSet.has(file) || files.some((other) => other.startsWith(`${file}/`));
export const isDirectory = (directory: string) =>
  files.some((file) => file.startsWith(`${directory}/`));
/** The files under `directory`, at any depth. */
export const filesIn = (directory: string) =>
  files.filter((file) => file.startsWith(`${directory}/`));
/** The names directly inside `directory`: its files and its folders. */
export function entries(directory: string): string[] {
  const names = new Set<string>();
  for (const file of filesIn(directory)) names.add(file.slice(directory.length + 1).split('/')[0]!);
  return [...names].sort();
}
/** `target` relative to the directory of `from`, normalized, with `/` separators. */
export const join = (from: string, target: string) =>
  path.posix.normalize(path.posix.join(path.posix.dirname(from), target));

/**
 * Where a scaffolding template lands: `tooling/scaffold/templates/feature/<path>.tmpl` is
 * `features/<name>/<path>`, and likewise for a collector, so templates keep the owners' rules.
 * Any other file is where it is.
 */
export function templated(file: string): string {
  const match = /^tooling\/scaffold\/templates\/(feature|collector)\/(.+?)(\.tmpl)?$/.exec(file);
  return match ? `${match[1]}s/template/${match[2]}` : file;
}

export type Layer = 'app' | 'core' | 'collector' | 'feature';
/**
 * What owns a file: the app, a core part, a collector or a feature. Tooling, ops and services
 * own nothing.
 */
export type Owner = { dir: string; layer: Layer; name: string };
const layers: Record<string, Layer> = {
  core: 'core',
  collectors: 'collector',
  features: 'feature',
};
export function ownerOf(file: string): Owner | undefined {
  const [top, name, rest] = file.split('/');
  if (top === 'app') return { dir: 'app', layer: 'app', name: 'app' };
  // Core's crate root and Cargo.toml sit beside its parts and belong to core as a whole.
  if (top === 'core' && name && rest === undefined)
    return { dir: 'core', layer: 'core', name: 'core' };
  const layer = layers[top!];
  // A file directly in collectors/ or features/ belongs to no single owner.
  return layer && name && rest !== undefined ? { dir: `${top}/${name}`, layer, name } : undefined;
}
/** Core is one crate: for dependencies and data, its parts are one owner. */
export const unitOf = (owner: Owner) => (owner.layer === 'core' ? 'core' : owner.dir);
/** Every owner directory, from the files present. */
export function owners(layer?: Layer): Owner[] {
  const found = new Map<string, Owner>();
  for (const file of files) {
    const owner = ownerOf(file);
    if (owner && owner.dir !== 'core' && (!layer || owner.layer === layer))
      found.set(owner.dir, owner);
  }
  return [...found.values()].sort((a, b) => a.dir.localeCompare(b.dir));
}
export const features = () => owners('feature');
export const collectors = () => owners('collector');

/** Test code: anything in a `tests/` folder, or a test or spec file anywhere. */
export const isTestFile = (file: string) =>
  /(^|\/)tests\//.test(file) || /\.(test|spec)\.tsx?$|_test\.py$/.test(file);
