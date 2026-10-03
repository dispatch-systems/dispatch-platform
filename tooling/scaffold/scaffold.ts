import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as prettier from 'prettier';

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
  'meals',
  'routedata',
  'shared',
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
const ownerRoots = ['core', 'collectors', 'features', 'app'];
const rustSources = (root: string) =>
  ownerRoots.flatMap((dir) => files(root, dir, /\.rs$/)).filter((file) => !/\/tests\//.test(file));

export const features = (root: string) =>
  directories(root, 'features').filter((name) => exists(root, `features/${name}/feature.rs`));
export const collectors = (root: string) =>
  directories(root, 'collectors').filter((name) => exists(root, `collectors/${name}/collector.rs`));
export const coreParts = (root: string) => directories(root, 'core');

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
/** A place after every agent kind, source, endpoint and term declared so far. */
export function nextAgentOrder(root: string) {
  let highest = 0;
  const declared = rustSources(root).filter((file) =>
    /^(core\/mcp|features\/[^/]+\/mcp)\//.test(file),
  );
  for (const file of declared)
    for (const match of read(root, file).matchAll(/\border:\s*(\d+)/g))
      highest = Math.max(highest, Number(match[1]));
  return (Math.floor(highest / 10) + 1) * 10;
}
/**
 * The next migration of `database`, after every owner's numbered SQL file for it. A migration
 * written as code has no file: `Registry::check` and the migrations rule catch a repeated id.
 */
export function nextMigration(root: string, database: string) {
  let highest = 0;
  for (const dir of ownerRoots)
    for (const file of files(root, dir, new RegExp(`/migrations/${database}/\\d{4}_[^/]+\\.sql$`)))
      highest = Math.max(highest, Number(path.basename(file).slice(0, 4)));
  return highest + 1;
}
/** The collections a collector reads: one directory each under `collections/`. */
export const collectionsOf = (root: string, site: string) =>
  directories(root, `collectors/${site}/collections`);
/** What a collector's connection supplies to the pages that require it. */
export function capabilitiesOf(root: string, site: string) {
  const source = read(root, `collectors/${site}/collector.rs`);
  const list = /fn capabilities\(&self\)[^{]*\{\s*&\[([^\]]*)\]/.exec(source)?.[1] ?? '';
  return [...list.matchAll(/"([^"]+)"/g)].map((match) => match[1]!);
}
/** The feature file that keeps `collection` already, if any: each collection has one keeper. */
export function keeperOf(root: string, collection: string) {
  const names = new RegExp(`\\b${collection}::(\\{[^}]*\\bJOB_KIND\\b|JOB_KIND\\b)`);
  return rustSources(root).find((file) => {
    if (!file.startsWith('features/')) return false;
    const source = read(root, file);
    return /impl Keeper for/.test(source) && names.test(source);
  });
}
/** A feature's page as its `frontend/feature.ts` declares it: its address and label. */
export function pageOf(root: string, feature: string) {
  const file = `features/${feature}/frontend/feature.ts`;
  if (!exists(root, file)) return undefined;
  const match = /id:\s*'([^']+)',\s*scope:\s*'dsp',\s*label:\s*'([^']+)'/.exec(read(root, file));
  return match ? { id: match[1]!, label: match[2]! } : undefined;
}
/** Whether core's frontend slots offer `slot` yet. */
export const offersSlot = (root: string, slot: string) =>
  new RegExp(`\\b${slot}\\?:`).test(read(root, 'core/shell/frontend/runtime/slots.ts'));

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
    if (char === ',' && depth === 0) (out.push(item.trim()), (item = ''));
    else item += char;
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
 * `dispatch-` ones, with the `features` named enabled.
 */
export function addDependency(
  text: string,
  crate: string,
  location: string,
  file: string,
  { section = 'dependencies', features = [] as string[] } = {},
) {
  const lines = text.split('\n');
  const header = lines.findIndex((line) => line.trim() === `[${section}]`);
  if (header < 0) throw new Error(`${file} has no [${section}]`);
  let end = lines.findIndex((line, index) => index > header && line.startsWith('['));
  if (end < 0) end = lines.length;
  const listed = lines.slice(header + 1, end);
  if (listed.some((line) => line.startsWith(`${crate} `)))
    throw new Error(`${file} already lists ${crate} in [${section}]`);
  const last = listed.findLastIndex((line) => line.startsWith('dispatch-'));
  const enabled = features.length
    ? `, features = [${features.map((feature) => `"${feature}"`).join(', ')}]`
    : '';
  lines.splice(header + 1 + last + 1, 0, `${crate} = { path = "${location}"${enabled} }`);
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
export const frontendList = 'app/frontend/features.ts';

/**
 * Adds an owner's frontend manifest to the app's list, after the last feature's or collector's
 * and before core's parts, whose order the sidebar follows.
 */
export function addFrontendFeature(text: string, local: string, from: string) {
  const imports = [...text.matchAll(/^import \{ feature as (\w+) \} from '([^']+)';$/gm)];
  const owned = imports.filter((match) => /\/(features|collectors)\//.test(match[2]!));
  const anchor = owned.at(-1) ?? imports.at(-1);
  if (!anchor) throw new Error(`${frontendList} imports no feature`);
  if (imports.some((match) => match[1] === local))
    throw new Error(`${frontendList} already has ${local}`);
  const line = `import { feature as ${local} } from '${from}';`;
  const at = anchor.index! + anchor[0].length;
  let next = `${text.slice(0, at)}\n${line}${text.slice(at)}`;
  const last = anchor[1]!;
  const list = /export const features[^=]*=\s*\[/.exec(next);
  if (!list) throw new Error(`${frontendList} has no features list`);
  const entry = new RegExp(`^(\\s*)${last},$`, 'm');
  const body = next.slice(list.index);
  const found = entry.exec(body);
  if (!found) throw new Error(`${frontendList}: ${last} is not in the list`);
  const position = list.index + found.index + found[0].length;
  next = `${next.slice(0, position)}\n${found[1]}${local},${next.slice(position)}`;
  return next;
}

// ---- Writing

export type Plan = {
  /** New files, by path from the root. */
  files: Map<string, string>;
  /** Existing files, whole, as they will read. */
  changes: Map<string, string>;
  /** What to do next, printed last. */
  notes: string[];
};
export const emptyPlan = (): Plan => ({ files: new Map(), changes: new Map(), notes: [] });
/** A file's content as the plan leaves it: changed already, or as it is. */
export const current = (plan: Plan, root: string, file: string) =>
  plan.changes.get(file) ?? read(root, file);

/**
 * Writes the plan into the root, or with `dryRun` prints what it would write and changes
 * nothing; `out` then receives every file it would write, whole, under the same paths.
 */
export function finish(plan: Plan, root: string, options: { dryRun: boolean; out?: string }) {
  for (const file of plan.files.keys())
    if (exists(root, file)) throw new Error(`${file} exists already`);
  const target = options.dryRun ? options.out : root;
  if (target)
    for (const [file, content] of [...plan.files, ...plan.changes]) {
      fs.mkdirSync(path.dirname(path.join(target, file)), { recursive: true });
      fs.writeFileSync(path.join(target, file), content);
    }
  const verb = options.dryRun ? 'Would write' : 'Wrote';
  const lines = [
    `${verb}:`,
    ...[...plan.files.keys()].sort().map((file) => `  ${file}`),
    `${options.dryRun ? 'Would change' : 'Changed'}:`,
    ...[...plan.changes.keys()].sort().map((file) => `  ${file}`),
  ];
  if (plan.notes.length) lines.push('Next:', ...plan.notes.map((note) => `  - ${note}`));
  process.stdout.write(`${lines.join('\n')}\n`);
}

/** Runs a generator's entry point: usage errors print the usage, others their message. */
export async function run(usage: string, main: (argv: string[]) => Promise<void>) {
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`${message}\n${error instanceof UsageError ? `\n${usage}\n` : ''}`);
    process.exitCode = 1;
  }
}
