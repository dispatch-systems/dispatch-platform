import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import * as prettier from 'prettier';
import { repositoryRoot } from './common.js';
import { exists, read } from './repository.js';
import { leftLines, markers, type Todo } from './todo.js';

/** Prettier's formatting, as the repository configures it, for what prettier formats. */
export async function format(file: string, content: string) {
  if (!/\.([jt]sx?|md|json|ya?ml|css)$/.test(file)) return content;
  const options = (await prettier.resolveConfig(path.join(repositoryRoot, file))) ?? {};
  return prettier.format(content, { ...options, filepath: file });
}
/**
 * Formats the plan's new Rust files with one rustfmt run, under the repository's toolchain. The
 * app's files it changes keep their own layout: an added line follows the lines around it.
 */
export function formatRust(plan: Plan) {
  const rust = [...plan.files.keys()].filter((file) => file.endsWith('.rs'));
  if (!rust.length) return;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-scaffold-rustfmt-'));
  try {
    for (const file of rust) {
      fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
      fs.writeFileSync(path.join(dir, file), plan.files.get(file)!);
    }
    const result = spawnSync(
      'rustfmt',
      ['--edition', '2024', ...rust.map((file) => path.join(dir, file))],
      {
        cwd: repositoryRoot,
        encoding: 'utf8',
      },
    );
    if (result.error) {
      plan.notes.push('rustfmt was not found: run `cargo fmt` once the files are written.');
      return;
    }
    // A template that is not valid Rust fails here, before anything is written.
    if (result.status !== 0)
      throw new Error(`The Rust it would write does not parse:\n${result.stderr}`);
    for (const file of rust) plan.files.set(file, fs.readFileSync(path.join(dir, file), 'utf8'));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * The last step for a new owner: the app's tests hold the whole product to snapshots, which it
 * changes. Rewriting them needs a build: `dispatchdev new` runs the command after writing.
 */
export const snapshotsNote =
  "`dispatchdev generate` rewrites the snapshots of the whole product the app's tests hold, " +
  'such as the catalog, the databases and the tools agents use, and names each file that ' +
  'changed: review them with the rest.';

export type Plan = {
  /** New files, by path from the root. */
  files: Map<string, string>;
  /** Existing files, whole, as they will read. */
  changes: Map<string, string>;
  /** What to do next, printed last. */
  notes: string[];
  /** Existing files it removes. */
  removed?: string[];
};
export const emptyPlan = (): Plan => ({ files: new Map(), changes: new Map(), notes: [] });
/** A file's content as the plan leaves it: changed already, or as it is. */
export const current = (plan: Plan, root: string, file: string) =>
  plan.changes.get(file) ?? read(root, file);
/** Plans an edit of an existing file, formatted; an edit that leaves it as it is, is none. */
export async function change(
  plan: Plan,
  root: string,
  file: string,
  edit: (text: string) => string,
) {
  const content = await format(file, edit(current(plan, root, file)));
  if (content !== read(root, file)) plan.changes.set(file, content);
  else plan.changes.delete(file);
}

/** What a generator made, or with `dryRun` would make. */
export type Made = {
  dryRun: boolean;
  /** The checkout it wrote into. */
  root: string;
  /** New files, by path from the root. */
  wrote: string[];
  /** Existing files it changed. */
  changed: string[];
  /** Existing files it removed. */
  removed: string[];
  /** What to do next. */
  next: string[];
  /** What it left for its author to write, each marker in what it wrote or changed. */
  todos: Todo[];
};

/**
 * Writes the plan into the root, or with `dryRun` changes nothing; `out` then receives every
 * file it would write, whole, under the same paths. Answers what it made.
 */
export function finish(plan: Plan, root: string, options: { dryRun: boolean; out?: string }): Made {
  for (const file of plan.files.keys())
    if (exists(root, file)) throw new Error(`${file} exists already`);
  const target = options.dryRun ? options.out : root;
  if (target)
    for (const [file, content] of [...plan.files, ...plan.changes]) {
      fs.mkdirSync(path.dirname(path.join(target, file)), { recursive: true });
      fs.writeFileSync(path.join(target, file), content);
    }
  if (!options.dryRun) for (const file of plan.removed ?? []) fs.rmSync(path.join(root, file));
  const changed = [...plan.changes.keys()];
  // Builds run with --locked, which refuses a crate, or a crate's dependencies, Cargo.lock does
  // not list.
  const crate = [...plan.files.keys(), ...changed].some((file) => file.endsWith('/Cargo.toml'));
  if (crate && options.dryRun)
    plan.notes.unshift(
      'Cargo.lock would list the crates as they now are, as `cargo update --workspace` does.',
    );
  else if (crate && exists(root, 'Cargo.lock')) {
    const before = read(root, 'Cargo.lock');
    if (lockWorkspace(root)) {
      if (read(root, 'Cargo.lock') !== before) changed.push('Cargo.lock');
    } else
      plan.notes.unshift(
        'Cargo.lock does not list the crates as they now are, and builds with --locked refuse ' +
          'it: run `cargo update --workspace`.',
      );
  }
  return {
    dryRun: options.dryRun,
    root,
    wrote: [...plan.files.keys()].sort(),
    changed: changed.sort(),
    removed: [...(plan.removed ?? [])].sort(),
    next: plan.notes,
    todos: [...plan.files, ...plan.changes]
      .sort(([a], [b]) => (a < b ? -1 : 1))
      .flatMap(([file, content]) => markers(file, content)),
  };
}
/**
 * What a generator made as people read it, then its next steps and what it left to write unless
 * `next` is false.
 */
export function described(made: Made, next = true) {
  const lines = [
    `${made.dryRun ? 'Would write' : 'Wrote'}:`,
    ...made.wrote.map((file) => `  ${file}`),
    `${made.dryRun ? 'Would change' : 'Changed'}:`,
    ...made.changed.map((file) => `  ${file}`),
    ...(made.removed.length
      ? [`${made.dryRun ? 'Would remove' : 'Removed'}:`, ...made.removed.map((file) => `  ${file}`)]
      : []),
  ];
  if (next) lines.push(...(made.next.length ? nextLines(made.next) : []), ...leftLines(made.todos));
  return lines.join('\n');
}
export const nextLines = (notes: string[]) => ['Next:', ...notes.map((note) => `  - ${note}`)];
/**
 * Has Cargo.lock list the workspace's crates as they now are, without the network: only the
 * workspace's own entries change, and every other crate keeps its locked version.
 */
function lockWorkspace(root: string) {
  const result = spawnSync('cargo', ['update', '--workspace', '--offline', '--quiet'], {
    cwd: root,
    encoding: 'utf8',
  });
  return !result.error && result.status === 0;
}
