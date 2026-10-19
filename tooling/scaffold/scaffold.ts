import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as prettier from 'prettier';
import { leftLines, markers, type Todo } from './todo.js';
import { marker } from './wire.js';

// What `new-feature.ts` and `new-collector.ts` share: their arguments, names, the templates,
// what they read from the repository, the edits that list a new owner in `app/`, and writing
// it all, or with `--dry-run` only saying what they would write.
//
// The generators write the crate layout of the Phase B brief: a crate per owner, its manifest
// as the crate root, `dispatch_core::` for core. The Rust paths they assume live in the
// templates, so a renamed module is a find-and-replace in `templates/`.

const here = path.dirname(fileURLToPath(import.meta.url));
/** The repository the scaffold belongs to; `--root` points the generators at another. */
export const repositoryRoot = path.resolve(here, '../..');
const templates = path.join(here, 'templates');

export class UsageError extends Error {}

// ---- Arguments

export type Arguments = {
  positional: string[];
  flags: Set<string>;
  options: Map<string, string>;
};
/** `--flag`, `--option value` or `--option=value`, and the rest in order. */
export function parseArguments(argv: string[], flags: string[], options: string[]): Arguments {
  const parsed: Arguments = { positional: [], flags: new Set(), options: new Map() };
  for (let index = 0; index < argv.length; index++) {
    const argument = argv[index]!;
    if (!argument.startsWith('--')) {
      parsed.positional.push(argument);
      continue;
    }
    const [key, inline] = argument.slice(2).split(/=(.*)/s, 2) as [string, string | undefined];
    if (flags.includes(key) && inline === undefined) parsed.flags.add(key);
    else if (options.includes(key)) {
      const value = inline ?? argv[++index];
      if (!value || value.startsWith('--')) throw new UsageError(`--${key} needs a value`);
      parsed.options.set(key, value);
    } else throw new UsageError(`Unknown option ${argument}`);
  }
  return parsed;
}

// ---- Names

export type Names = {
  /** The directory's name, snake_case: `driver_match`. */
  name: string;
  /** In addresses and file names: `driver-match`. */
  slug: string;
  pascal: string;
  camel: string;
  constant: string;
  label: string;
  /** The crate: `dispatch-driver-match`, used as `dispatch_driver_match`. */
  crate: string;
  ident: string;
};
const NAME = /^[a-z][a-z0-9]*(_[a-z0-9]+)*$/;
// Names the structure rules retire, and the top level's own.
const RESERVED = [
  'agents',
  'app',
  'auth',
  'browsers',
  'collectors',
  'core',
  'drivers',
  'features',
  'mcp',
  'meals',
  'routedata',
  'workforce',
];
export function names(name: string, label?: string): Names {
  if (!NAME.test(name))
    throw new UsageError(`${name} is not a name: use lowercase words joined by _, as driver_match`);
  if (RESERVED.includes(name)) throw new UsageError(`${name} is a retired or reserved name`);
  const words = name.split('_');
  const capital = (word: string) => word[0]!.toUpperCase() + word.slice(1);
  return {
    name,
    slug: words.join('-'),
    pascal: words.map(capital).join(''),
    camel: words[0]! + words.slice(1).map(capital).join(''),
    constant: name.toUpperCase(),
    label: label ?? words.map(capital).join(' '),
    crate: `dispatch-${words.join('-')}`,
    ident: `dispatch_${name}`,
  };
}

// ---- Templates

export type Values = Record<string, string | number | boolean>;
const STANDALONE = /^[ \t]*(\{\{[#^/]\w+\}\})[ \t]*\r?\n/gm;
const TAG = /\{\{([#^/]?)(\w+)\}\}/g;
/**
 * Fills a template: `{{value}}`, `{{#flag}}…{{/flag}}` while `flag` is set, `{{^flag}}…{{/flag}}`
 * while it is not. A line holding only a section tag leaves no line behind. Every name must have
 * a value, so a misspelt one fails here rather than in the output.
 */
export function render(source: string, values: Values, file = 'template'): string {
  const text = source.replace(STANDALONE, '$1');
  const value = (name: string) => {
    if (!(name in values)) throw new Error(`${file}: no value for {{${name}}}`);
    return values[name]!;
  };
  const open: { name: string; shown: boolean }[] = [];
  const shown = () => open.every((section) => section.shown);
  let output = '';
  let index = 0;
  for (const match of text.matchAll(TAG)) {
    const [tag, sigil, name] = match as unknown as [string, string, string];
    if (shown()) output += text.slice(index, match.index);
    index = match.index + tag.length;
    if (sigil === '#' || sigil === '^') {
      const set = Boolean(value(name));
      open.push({ name, shown: sigil === '#' ? set : !set });
    } else if (sigil === '/') {
      const section = open.pop();
      if (section?.name !== name)
        throw new Error(`${file}: {{/${name}}} closes ${section ? section.name : 'nothing'}`);
    } else {
      const filled = value(name);
      if (shown()) output += String(filled);
    }
  }
  if (open.length) throw new Error(`${file}: {{#${open.at(-1)!.name}}} is never closed`);
  return output + text.slice(index);
}
/** `templates/<name>.tmpl`, filled. */
export function template(name: string, values: Values) {
  return render(fs.readFileSync(path.join(templates, `${name}.tmpl`), 'utf8'), values, name);
}

// ---- Formatting

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

// ---- What the repository holds

export const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');
export const exists = (root: string, file: string) => fs.existsSync(path.join(root, file));
/** The directories in `dir`, sorted; none when it does not exist. */
export function directories(root: string, dir: string) {
  if (!exists(root, dir)) return [];
  return fs
    .readdirSync(path.join(root, dir), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
}
/** Every file under `dir` whose path matches, relative to the root. */
export function files(root: string, dir: string, pattern: RegExp): string[] {
  if (!exists(root, dir)) return [];
  return fs
    .readdirSync(path.join(root, dir), { recursive: true, encoding: 'utf8' })
    .map((name) => path.posix.join(dir, name.split(path.sep).join('/')))
    .filter((file) => pattern.test(file) && !/(^|\/)(node_modules|target)\//.test(file))
    .sort();
}
const ownerRoots = ['core', 'collectors', 'features', 'mcp', 'app'];
const rustSources = (root: string) =>
  ownerRoots.flatMap((dir) => files(root, dir, /\.rs$/)).filter((file) => !/\/tests\//.test(file));

export const features = (root: string) =>
  directories(root, 'features').filter((name) => exists(root, `features/${name}/feature.rs`));
export const collectors = (root: string) =>
  directories(root, 'collectors').filter((name) => exists(root, `collectors/${name}/collector.rs`));
export const coreParts = (root: string) => directories(root, 'core');

/** The next free tens after every feature's place. */
export function nextPlace(root: string) {
  let highest = 0;
  for (const name of features(root))
    for (const match of read(root, `features/${name}/feature.rs`).matchAll(/^\s*place:\s*(\d+),/gm))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/** The next free tens in the one order every list of permissions follows. */
export function nextPermissionOrder(root: string) {
  let highest = 0;
  for (const file of rustSources(root))
    for (const match of read(root, file).matchAll(
      /\bperm\(\s*"[^"]*",\s*"[^"]*",\s*(\d+)\s*,?\s*\)/g,
    ))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/** Whether lucide-react, which draws the dashboard's icons, has an icon of this name. */
export function isIcon(name: string) {
  const icons = createRequire(import.meta.url)('lucide-react') as Record<string, unknown>;
  return /^[A-Z][A-Za-z0-9]*$/.test(name) && !name.endsWith('Icon') && name in icons;
}
/**
 * The order after a feature's own permissions', for those added to them: its next three are
 * free, so a view, collect and manage permission each keep one no other feature holds.
 */
export function ownPermissionOrder(root: string, feature: string) {
  const orders = (text: string) =>
    [...text.matchAll(/\bperm\(\s*"[^"]*",\s*"[^"]*",\s*(\d+)\s*,?\s*\)/g)].map((match) =>
      Number(match[1]),
    );
  const own = orders(read(root, `features/${feature}/feature.rs`));
  if (!own.length) return nextPermissionOrder(root);
  const used = new Set(rustSources(root).flatMap((file) => orders(read(root, file))));
  let order = Math.max(...own) + 1;
  while ([0, 1, 2].some((step) => used.has(order + step))) order++;
  return order;
}
/** The next free tens in the one order the sources of people's IDs follow. */
export function nextSourceOrder(root: string) {
  let highest = 0;
  for (const file of rustSources(root))
    for (const match of read(root, file).matchAll(/\bSourceInfo \{[^}]*?\border:\s*(\d+),/g))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/** The next free tens in the one order the kinds of data that name people follow. */
export function nextPeopleOrder(root: string) {
  let highest = 0;
  for (const file of rustSources(root))
    for (const match of read(root, file).matchAll(/\bKindInfo \{[^}]*?\border:\s*(\d+),/g))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/**
 * The sources of the IDs people are known by, each with the collector that declares it, as
 * its `SOURCE`: Amazon's transporter IDs are Cortex's.
 */
export function peopleSources(root: string) {
  return collectors(root).flatMap((site) => {
    const source = read(root, `collectors/${site}/collector.rs`);
    const id = /\bSOURCE: Source = Source::new\(&SourceInfo \{\s*id: "([^"]+)"/.exec(source)?.[1];
    return id ? [{ id, site }] : [];
  });
}
/** The collections a collector reads: one directory each under `collections/`. */
export const collectionsOf = (root: string, site: string) =>
  directories(root, `collectors/${site}/collections`);
/** What a collector's connection supplies to the pages that require it. */
export function capabilitiesOf(root: string, site: string) {
  const source = read(root, `collectors/${site}/collector.rs`);
  const list = /fn capabilities\(&self\)[^{]*\{\s*&\[([^\]]*)\]/.exec(source)?.[1] ?? '';
  return [...list.matchAll(/\bid:\s*"([^"]+)"/g)].map((match) => match[1]!);
}
/**
 * The feature file that keeps `site`'s `collection` already, if any: each collection has one
 * keeper. Another collector may have a collection of the same name.
 */
export function keeperOf(root: string, site: string, collection: string) {
  const names = new RegExp(`\\b${collection}::(\\{[^}]*\\bJOB_KIND\\b|JOB_KIND\\b)`);
  const crate = new RegExp(`\\bdispatch_${site}\\b`);
  return rustSources(root).find((file) => {
    if (!file.startsWith('features/')) return false;
    const source = read(root, file);
    return /impl Keeper for/.test(source) && crate.test(source) && names.test(source);
  });
}
/** A feature's page as its `frontend/feature.ts` declares it: its address and label. */
export function pageOf(root: string, feature: string) {
  const file = `features/${feature}/frontend/feature.ts`;
  if (!exists(root, file)) return undefined;
  const source = read(root, file);
  const match = /id:\s*'([^']+)',\s*scope:\s*'dsp',\s*label:\s*'([^']+)'/.exec(source);
  return match
    ? { id: match[1]!, label: match[2]!, hostsSettings: /\bhostsSettings: true\b/.test(source) }
    : undefined;
}

// ---- Edits that list a new owner in app/

/** The index of the bracket that closes the one at `start` in Rust, skipping string literals. */
function closing(text: string, start: number) {
  const pairs: Record<string, string> = { '[': ']', '{': '}', '(': ')' };
  const stack: string[] = [];
  for (let index = start; index < text.length; index++) {
    const char = text[index]!;
    if (char === '"') {
      for (index++; index < text.length && text[index] !== char; index++)
        if (text[index] === '\\') index++;
    } else if (pairs[char]) stack.push(pairs[char]!);
    else if (char === stack.at(-1)) {
      stack.pop();
      if (!stack.length) return index;
    }
  }
  throw new Error('unbalanced brackets');
}
/** The items of a one-line list's insides, split at its top-level commas. */
function listItems(inside: string) {
  const out: string[] = [];
  let depth = 0;
  let item = '';
  for (const char of inside) {
    if ('[{('.includes(char)) depth++;
    if (']})'.includes(char)) depth--;
    if (char === ',' && depth === 0) {
      out.push(item.trim());
      item = '';
    } else item += char;
  }
  return [...out, item.trim()].filter(Boolean);
}
/**
 * Adds `entry` as the last item of the list that `opener` opens, indented as the items before
 * it are. `opener` must end at the list's opening bracket. A list rustfmt keeps on one line,
 * being short, is written out one item a line, as rustfmt writes it once it is longer.
 */
export function appendToList(text: string, opener: RegExp, entry: string, file: string) {
  const found = opener.exec(text);
  if (!found) throw new Error(`${file}: cannot find ${opener}; add ${entry} by hand`);
  const start = found.index + found[0].length - 1;
  const end = closing(text, start);
  const lineStart = text.lastIndexOf('\n', end - 1) + 1;
  const inside = text.slice(start + 1, end);
  if (lineStart <= start && !/["/]/.test(inside)) {
    const indent = text.slice(text.lastIndexOf('\n', start) + 1).match(/^\s*/)![0];
    const listed = listItems(inside);
    if (listed.includes(entry.trim().replace(/,$/, '')))
      throw new Error(`${file} already lists ${entry.trim()}`);
    const lines = [...listed.map((item) => `${item},`), entry].map(
      (line) => `${indent}    ${line}\n`,
    );
    return `${text.slice(0, start + 1)}\n${lines.join('')}${indent}${text.slice(end)}`;
  }
  if (lineStart <= start || text.slice(lineStart, end).trim())
    throw new Error(
      `${file}: the list ${opener} opens is not one item a line; add ${entry} by hand`,
    );
  const items = text.slice(start + 1, lineStart).split('\n');
  const indent =
    items
      .reverse()
      .find((line) => line.trim())
      ?.match(/^\s*/)?.[0] ??
    `${text.slice(text.lastIndexOf('\n', found.index) + 1).match(/^\s*/)![0]}    `;
  if (text.slice(start, end).includes(entry.trim()))
    throw new Error(`${file} already lists ${entry.trim()}`);
  return `${text.slice(0, lineStart)}${indent}${entry}\n${text.slice(lineStart)}`;
}
/**
 * Adds a path dependency to a crate's `[dependencies]`, or the `section` named, after its other
 * `dispatch-` ones and above the features' list the wiring writes, with the `features` named
 * enabled.
 */
export function addDependency(
  text: string,
  crate: string,
  location: string,
  file: string,
  { section = 'dependencies', features = [] as string[], optional = false } = {},
) {
  const lines = text.split('\n');
  const header = lines.findIndex((line) => line.trim() === `[${section}]`);
  if (header < 0) throw new Error(`${file} has no [${section}]`);
  let end = lines.findIndex((line, index) => index > header && line.startsWith('['));
  if (end < 0) end = lines.length;
  const listed = lines.slice(header + 1, end);
  if (listed.some((line) => line.startsWith(`${crate} `)))
    throw new Error(`${file} already lists ${crate} in [${section}]`);
  const wired = listed.indexOf(marker);
  const last = (wired < 0 ? listed : listed.slice(0, wired)).findLastIndex((line) =>
    line.startsWith('dispatch-'),
  );
  const enabled = features.length
    ? `, features = [${features.map((feature) => `"${feature}"`).join(', ')}]`
    : '';
  const kept = optional ? ', optional = true' : '';
  lines.splice(header + 1 + last + 1, 0, `${crate} = { path = "${location}"${enabled}${kept} }`);
  return lines.join('\n');
}
/** Has the app's `feature` enable the one of that name on `crate` as well. */
export function forwardFeature(text: string, feature: string, crate: string, file: string) {
  const line = new RegExp(`^${feature} = \\[([^\\]]*)\\]$`, 'm').exec(text);
  if (!line) throw new Error(`${file} has no ${feature} feature; add ${crate}/${feature} by hand`);
  const enabled = [...line[1]!.matchAll(/"([^"]+)"/g)].map((match) => match[1]!);
  if (enabled.includes(`${crate}/${feature}`))
    throw new Error(`${file} already enables ${crate}/${feature}`);
  const list = [...enabled, `${crate}/${feature}`].sort().map((item) => `"${item}"`);
  return text.replace(line[0], `${feature} = [${list.join(', ')}]`);
}
/** Adds `dir` to the workspace's members unless one of them, or a glob, covers it. */
export function addWorkspaceMember(text: string, dir: string) {
  const list = /members\s*=\s*\[([^\]]*)\]/.exec(text);
  if (!list) throw new Error(`Cargo.toml has no workspace members; add "${dir}" by hand`);
  const members = [...list[1]!.matchAll(/"([^"]+)"/g)].map((match) => match[1]!);
  const covered = members.some(
    (member) =>
      member === dir || (member.endsWith('/*') && path.posix.dirname(dir) === member.slice(0, -2)),
  );
  if (covered) return text;
  const end = list.index + list[0].length - 1;
  const separator = list[1]!.trim() ? ', ' : '';
  return `${text.slice(0, end).trimEnd()}${separator}"${dir}"${text.slice(end)}`;
}
/** The file that holds `pattern`, of `candidates`: Phase B may move the app's lists. */
export function holding(root: string, candidates: string[], pattern: RegExp) {
  const file = candidates.find(
    (candidate) => exists(root, candidate) && pattern.test(read(root, candidate)),
  );
  if (!file) throw new Error(`None of ${candidates.join(', ')} holds ${pattern}`);
  return file;
}
/** Where the app's backend lists every collector and feature. */
export const appBackend = ['app/backend/features.rs', 'app/backend/lib.rs'];
export const routeInventory = 'app/tests/backend/integration/http_routes.rs';
/**
 * The last step for a new owner: the app's tests hold the whole product to snapshots, which it
 * changes. Rewriting them needs a build: `dispatchdev new` runs the command after writing.
 */
export const snapshotsNote =
  "`dispatchdev generate` rewrites the snapshots of the whole product the app's tests hold, " +
  'such as the catalog, the databases and the tools agents use, and names each file that ' +
  'changed: review them with the rest.';
/** The app's test that writes every owner's API types to TypeScript, which lists each type. */
export const typescriptExport = 'app/tests/backend/export.rs';

// ---- Writing

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

// ---- Generators

/** A kind of thing `dispatchdev new` makes: its usage, its options and how it plans one. */
export type Generator = {
  kind: string;
  /** What it makes, in a line. */
  about: string;
  usage: string;
  flags: string[];
  options: string[];
  plan: (root: string, argv: string[]) => Promise<{ plan: Plan; args: Arguments }>;
  /** Whether `dispatchdev generate` runs once it has written. */
  generates: boolean;
};
/** Whether the arguments only ask for the usage: none, or --help. */
export const asksUsage = (argv: string[]) =>
  argv.length === 0 || argv.includes('--help') || argv.includes('-h');
/** Plans what `argv` asks of the generator and writes it, or with --dry-run only says so. */
export async function make(generator: Generator, argv: string[]): Promise<Made> {
  const options = parseArguments(argv, generator.flags, generator.options);
  const root = path.resolve(options.options.get('root') ?? repositoryRoot);
  const { plan, args } = await generator.plan(root, argv);
  const dryRun = args.flags.has('dry-run');
  if (args.options.has('out') && !dryRun) throw new UsageError('--out goes with --dry-run');
  return finish(plan, root, { dryRun, out: args.options.get('out') });
}
/** The message of a failure, with the usage after a usage error. */
export const failed = (error: unknown, usage: string) =>
  `${error instanceof Error ? error.message : String(error)}\n${error instanceof UsageError ? `\n${usage}\n` : ''}`;

/**
 * Runs a generator as its own script: the usage, or what it made, as JSON with --json, or why it
 * failed.
 */
export async function run(generator: Generator) {
  const argv = process.argv.slice(2).filter((arg) => arg !== '--json');
  const json = argv.length < process.argv.length - 2;
  try {
    if (asksUsage(argv)) process.stdout.write(`${generator.usage}\n`);
    else {
      const made = await make(generator, argv);
      process.stdout.write(`${json ? JSON.stringify({ made }) : described(made)}\n`);
    }
  } catch (error) {
    process.stderr.write(failed(error, generator.usage));
    process.exitCode = 1;
  }
}
