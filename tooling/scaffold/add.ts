import { spawnSync } from 'node:child_process';
import { renderFeature, settingsNote, showing } from './new-feature.js';
import {
  UsageError,
  exists,
  features,
  format,
  names,
  formatRust,
  collectors,
  files,
  pageOf,
  peopleSources,
  parseArguments,
  read,
  repositoryRoot,
  type Generator,
  type Plan,
} from './scaffold.js';

// `dispatchdev add <piece> --to <feature>`: a piece `dispatchdev new feature` writes, added to a
// feature that exists. The piece is rendered as the feature's own templates would write it, and
// what it adds to the feature's manifests is merged into them: imports, page declarations,
// constants, slot entries, fields and permissions. An entry joins a list the feature fills
// already; a field or function it declares already is left to its author, named in what to do
// next.

export const usage = `Usage: dispatchdev add <piece> --to <feature> [options]

  page                a page in a DSP's sidebar
  tab-of <feature>    a tab of another feature's page
  settings            a section of a DSP's Settings, on a page of its own, that its backend
                      declares and reads: for those who manage it
  api                 api/ with one endpoint behind the view permission
  people <source>     who --table <table>, in the DSP's database or a collector's, names, for
                      Driver Match
  contract <name>     what it exposes to other features and agent tools, named for what it
                      offers: its contract crate in contract/, what fulfils it, and a test
  --shown             with contract, one a page shows: its name and the permission that shows
                      what it keeps, the feature's view, which offered(<name>) reads
  --icon <name>       its page's icon, from lucide-react
  --dry-run           print what it would write and change nothing
  --out <dir>         with --dry-run, write the files under <dir> instead
  --root <dir>        the repository to write into (default: this one)`;

/**
 * Each piece: whether it takes a value, its README line's label, and the permissions it uses,
 * which it adds where the feature lacks them.
 */
const PIECES: Record<string, { valued: boolean; readme: string; uses: string[] }> = {
  page: { valued: false, readme: 'Page', uses: ['view'] },
  'tab-of': { valued: true, readme: 'Tab', uses: [] },
  settings: { valued: false, readme: 'Settings', uses: ['manage'] },
  api: { valued: false, readme: 'API', uses: ['view'] },
  people: { valued: true, readme: 'People', uses: [] },
  contract: { valued: true, readme: 'Exposed', uses: [] },
};
const FLAGS = ['dry-run', 'shown'];
const OPTIONS = ['to', 'table', 'icon', 'out', 'root'];
/** The manifest's fields a piece adds: lists join the feature's, others are added whole. */
const FIELDS = [
  'permissions',
  'routes',
  'settings',
  'people',
  'depends_on',
  'integrations',
  'exposes',
];

/**
 * The index just past the comment or string starting at `at`, or `at` if none does. A `'` opens
 * a string unless it follows `&`, `<` or a letter, as Rust's `&'static` does.
 */
function past(text: string, at: number) {
  const char = text[at]!;
  if (text.startsWith('//', at)) {
    const end = text.indexOf('\n', at);
    return end < 0 ? text.length : end;
  }
  if (text.startsWith('/*', at)) return text.indexOf('*/', at) + 2;
  const quote =
    char === '"' || char === '`' || (char === "'" && !/[&<\w]/.test(text[at - 1] ?? ''));
  if (!quote) return at;
  let index = at + 1;
  for (; index < text.length && text[index] !== char; index++) if (text[index] === '\\') index++;
  return index + 1;
}
/** Walks `text` from `start` to `end`, past comments and strings, calling `each` on the rest. */
function walk(
  text: string,
  start: number,
  end: number,
  each: (char: string, at: number) => boolean,
) {
  for (let index = start; index < end;) {
    const next = past(text, index);
    if (next !== index) {
      index = next;
      continue;
    }
    if (each(text[index]!, index)) return index;
    index++;
  }
  return -1;
}
/** The index of the bracket closing the one at `at`. */
function closing(text: string, at: number) {
  const open = text[at]!;
  const close = ({ '[': ']', '{': '}', '(': ')' } as Record<string, string>)[open]!;
  let depth = 0;
  const found = walk(text, at, text.length, (char) =>
    char === open ? (depth++, false) : char === close && --depth === 0,
  );
  if (found < 0) throw new Error(`unbalanced ${open}`);
  return found;
}
/** Where the entry or item starting at `start` ends: its comma at the top level, or `end`. */
function entryEnd(text: string, start: number, end: number) {
  let depth = 0;
  const comma = walk(text, start, end, (char) => {
    if ('[{('.includes(char)) depth++;
    else if (']})'.includes(char)) depth--;
    return char === ',' && depth === 0;
  });
  return comma < 0 ? end : comma;
}
/** The entries of the object or struct literal whose `{` is at `open`: each key and its text. */
function entries(text: string, open: number) {
  const end = closing(text, open);
  const found: { name: string; start: number; end: number; text: string }[] = [];
  let index = open + 1;
  while (index < end) {
    const match = /^(?:\s|\/\/[^\n]*\n)*(?:get )?(\w+)\s*[:(]/.exec(text.slice(index, end));
    if (!match) break;
    const at = entryEnd(text, index + match[0].length - 1, end);
    found.push({ name: match[1]!, start: index, end: at + 1, text: text.slice(index, at + 1) });
    index = at + 1;
  }
  return { entries: found, end };
}
/** The items of a list's `[...]` text, at its top level. */
function items(list: string) {
  const open = list.indexOf('[');
  const close = closing(list, open);
  const found: string[] = [];
  for (let index = open + 1; index < close;) {
    const end = entryEnd(list, index, close);
    found.push(list.slice(index, end).trim());
    index = end + 1;
  }
  return found.filter(Boolean);
}
/** `text` with `additions` appended to the list whose `[` is at `open`. */
function appended(text: string, open: number, additions: string[]) {
  const close = closing(text, open);
  const before = text.slice(0, close).trimEnd();
  const comma = before.endsWith('[') || before.endsWith(',') ? '' : ',';
  return `${before}${comma}\n${additions.join(',\n')},\n${text.slice(close)}`;
}

// ---- Rust

/** The whole top-level statements beginning with `start`. */
function statements(source: string, start: RegExp) {
  const found: string[] = [];
  const lines = source.split('\n');
  for (let index = 0; index < lines.length; index++) {
    if (!start.test(lines[index]!)) continue;
    let statement = lines[index]!;
    while (!statement.trimEnd().endsWith(';') && index + 1 < lines.length)
      statement += `\n${lines[++index]}`;
    found.push(statement);
  }
  return found;
}
/** The names a `use` brings, each with the path it comes from, however deep its tree. */
function useNames(statement: string): [string, string][] {
  const tree = statement
    .replace(/\s+/g, ' ')
    .replace(/^(pub(\([^)]*\))? )?use /, '')
    .replace(/;$/, '')
    .trim();
  return treeNames(tree, '');
}
function treeNames(tree: string, prefix: string): [string, string][] {
  const brace = tree.indexOf('{');
  if (brace < 0) {
    const parts = tree.split('::');
    const path = [prefix, ...parts.slice(0, -1)].filter(Boolean).join('::');
    const leaf = parts.at(-1)!.split(' as ').at(-1)!.trim();
    return [[path, leaf === 'self' ? path.split('::').at(-1)! : leaf]];
  }
  const base = [prefix, tree.slice(0, brace).replace(/::$/, '')].filter(Boolean).join('::');
  const inner = tree.slice(brace + 1, closing(tree, brace));
  return items(`[${inner}]`).flatMap((item) => treeNames(item, base));
}
/** `text` importing `name` from `from`: in a group of that path's names if one is there. */
function withImport(text: string, from: string, name: string) {
  const last = from.split('::').at(-1)!;
  for (const statement of statements(text, /^use /)) {
    if (!useNames(statement).some(([path]) => path === from)) continue;
    const group = statement.indexOf(`${last}::{`);
    if (group >= 0) {
      const at = group + last.length + 3;
      return text.replace(statement, `${statement.slice(0, at)}${name}, ${statement.slice(at)}`);
    }
    const one = new RegExp(`^use ${from}::(\\w+);$`).exec(statement.trim());
    if (one) return text.replace(statement, `use ${from}::{${one[1]}, ${name}};`);
  }
  const lastUse = statements(text, /^use /).at(-1);
  if (lastUse) {
    const at = text.indexOf(lastUse) + lastUse.length;
    return `${text.slice(0, at)}\nuse ${from}::${name};${text.slice(at)}`;
  }
  const at = text.indexOf('\npub const FEATURE');
  return `${text.slice(0, at)}\nuse ${from}::${name};\n${text.slice(at)}`;
}
/** `text` with `line` after the last line `pattern` matches, or else before `before`'s first. */
function placed(text: string, line: string, pattern: RegExp, before: RegExp) {
  const last = [...text.matchAll(pattern)].at(-1);
  if (last) {
    const at = last.index! + last[0].length + 1;
    return `${text.slice(0, at)}${line}\n${text.slice(at)}`;
  }
  const first = before.exec(text);
  const at = first ? first.index : text.indexOf('\npub const FEATURE') + 1;
  return `${text.slice(0, at)}${line}\n\n${text.slice(at)}`;
}
/** The feature's manifest with what the piece's adds; what it can't add goes in `left`. */
export function mergeRust(
  target: string,
  piece: string,
  left: string[],
  uses: string[],
  writes: (module: string) => boolean,
) {
  let text = target;
  const struct = (source: string) => source.indexOf('{', source.indexOf('pub const FEATURE'));
  const id = (item: string) => /"([^"]+)"/.exec(item)?.[1] ?? item;
  // Its fields: a list's entries join the feature's list, and another field is added whole.
  for (const field of entries(piece, struct(piece)).entries) {
    if (!FIELDS.includes(field.name)) continue;
    const have = entries(text, struct(text)).entries.find((each) => each.name === field.name);
    if (!have && field.name === 'permissions') {
      const used = items(field.text).filter((item) =>
        uses.some((use) => id(item).endsWith(`.${use}`)),
      );
      if (!used.length) continue;
      const known = new Set(used.map(id));
      const list = used.map((item) =>
        item.replace(/\s*\.implies\(&\[([^\]]*)\]\)/, (_, implied: string) => {
          const kept = items(`[${implied}]`).filter((each) => known.has(id(each)));
          return kept.length ? `.implies(&[${kept.join(', ')}])` : '';
        }),
      );
      const at = text.search(/\n\s*\.\.feature\("/);
      text = `${text.slice(0, at)}\n    permissions: &[${list.join(', ')}],${text.slice(at)}`;
      continue;
    }
    if (!have) {
      const at = text.search(/\n\s*\.\.feature\("/);
      text = `${text.slice(0, at)}\n${field.text.replace(/^\n+/, '').trimEnd()}${text.slice(at)}`;
      continue;
    }
    if (!field.text.includes('&[')) {
      left.push(`its manifest declares ${field.name} already: ${field.text.trim()}`);
      continue;
    }
    const there = items(have.text).map(id);
    // Of the permissions, those the piece uses, implying only permissions the feature has.
    let added = items(field.text).filter(
      (item) =>
        !there.includes(id(item)) &&
        (field.name !== 'permissions' || uses.some((use) => id(item).endsWith(`.${use}`))),
    );
    const known = new Set([...there, ...added.map(id)]);
    added = added.map((item) =>
      item.replace(/\s*\.implies\(&\[([^\]]*)\]\)/, (_, list: string) => {
        const kept = items(`[${list}]`).filter((each) => known.has(id(each)));
        return kept.length ? `.implies(&[${kept.join(', ')}])` : '';
      }),
    );
    if (added.length) text = appended(text, text.indexOf('&[', have.start) + 1, added);
  }
  // Its modules first, then its re-exports, each after the feature's own.
  for (const line of statements(piece, /^(pub(\([^)]*\))? )?mod \w+;/)) {
    const module = /mod (\w+);/.exec(line)![1];
    if (new RegExp(`^(pub(\\([^)]*\\))? )?mod ${module};`, 'm').test(text)) continue;
    // Among its own modules, never after one an attribute marks, as its test module.
    text = placed(
      text,
      line,
      /^(?<!#\[[^\n]*\n)(?:pub(?:\([^)]*\))? )?mod \w+;$/gm,
      /^(pub )?use /m,
    );
  }
  for (const line of statements(piece, /^pub use /)) {
    // A re-export of what the piece writes, never of what it leaves out.
    const from = /^pub use ([\w:]+)::/.exec(line)?.[1];
    if (text.includes(line) || (from && !writes(from.replace(/::/g, '/')))) continue;
    text = placed(text, line, /^pub use [^;]*;$/gms, /^use /m);
  }
  // Items after the manifest, as its API types' export: added whole unless one is there.
  const tail = piece.slice(closing(piece, struct(piece)) + 2).trim();
  for (const item of tail.split(/\n\n+/).filter(Boolean)) {
    const name = /\bfn (\w+)/.exec(item)?.[1];
    if (name && new RegExp(`\\bfn ${name}\\b`).test(text))
      left.push(`its manifest has ${name}() already: give it what the piece's adds`);
    else if (!text.includes(item)) text = `${text.trimEnd()}\n\n${item}\n`;
  }
  // Names it now uses and doesn't import, each from the path the piece imports it from.
  const imported = new Set(
    statements(text, /^use /).flatMap((line) => useNames(line).map(([, n]) => n)),
  );
  const body = text.replace(/^use [^;]*;$/gms, '').replace(/\/\/[^\n]*/g, '');
  for (const [from, name] of statements(piece, /^use /).flatMap(useNames))
    if (!imported.has(name) && new RegExp(`\\b${name}\\b`).test(body)) {
      text = withImport(text, from, name);
      imported.add(name);
    }
  return text;
}

// ---- TypeScript

type Import = { from: string; type: boolean; names: string[]; whole: string };
/** A module's imports: each with the names it imports, if it imports names. */
function imports(source: string): Import[] {
  return [...source.matchAll(/^import (type )?(\{[^}]*\}|[^;{]*?) from '([^']+)';$/gm)].map(
    (match) => ({
      from: match[3]!,
      type: Boolean(match[1]),
      names: match[2]!.startsWith('{') ? items(`[${match[2]!.slice(1, -1)}]`) : [],
      whole: match[0],
    }),
  );
}
/** `text` importing what `piece` imports too: a name joins an import of its module. */
function mergeImports(text: string, piece: string) {
  const bare = (name: string) => name.replace(/^type /, '');
  for (const wanted of imports(piece)) {
    const from = imports(text).filter((each) => each.from === wanted.from);
    if (from.some((each) => each.whole === wanted.whole)) continue;
    const named =
      from.find((each) => each.names.length && !each.type) ??
      from.find((each) => each.names.length);
    if (named && wanted.names.length) {
      const have = named.names.map(bare);
      const missing = wanted.names
        .filter((name) => !have.includes(bare(name)))
        .map((name) => (wanted.type && !named.type ? `type ${name}` : name));
      if (!missing.length) continue;
      text = text.replace(
        named.whole,
        `import ${named.type ? 'type ' : ''}{ ${[...named.names, ...missing].join(', ')} } from '${wanted.from}';`,
      );
    } else if (!from.length || !wanted.names.length) {
      const last = imports(text).at(-1);
      const at = last ? text.indexOf(last.whole) + last.whole.length : 0;
      text = `${text.slice(0, at)}\n${wanted.whole}${text.slice(at)}`;
    }
  }
  return text;
}
/** The feature's frontend manifest with what the piece's adds. */
export function mergeTypeScript(target: string, piece: string, pascal: string) {
  // A constant of the piece's that the manifest has under the same name, differently, takes
  // the feature's name: loadSettings becomes loadTimecardSettings.
  for (const [statement, name] of piece.matchAll(
    /^const (\w+) =[\s\S]*?;\n(?=\n|const |export )/gm,
  )) {
    const theirs = new RegExp(`^const ${name} =[\\s\\S]*?;\\n`, 'm').exec(target)?.[0];
    if (!theirs || theirs.trim() === statement.trim()) continue;
    const renamed = name!.replace(/^load/, `load${pascal}`);
    piece = piece.replace(
      new RegExp(`\\b${name}\\b`, 'g'),
      renamed === name ? `${name}${pascal}` : renamed,
    );
  }
  let text = mergeImports(target, piece);
  const manifest = (source: string) =>
    source.indexOf('{', source.indexOf('export const feature: FrontendFeature'));
  // Its pages join DspPages.
  for (const [, page] of piece.matchAll(/interface DspPages \{\s*([^}]*?)\s*\}/g)) {
    if (text.includes(page!)) continue;
    const declared = /interface DspPages \{\n/.exec(text);
    if (declared) {
      const at = declared.index + declared[0].length;
      text = `${text.slice(0, at)}    ${page}\n${text.slice(at)}`;
    } else {
      const block = /declare module [^{]+\{[\s\S]*?\n\}\n/.exec(piece)![0];
      const at = text.indexOf('\nexport const feature');
      text = `${text.slice(0, at)}\n${block}${text.slice(at)}`;
    }
  }
  // Its constants, before the manifest.
  for (const [statement, name] of piece.matchAll(
    /^const (\w+) =[\s\S]*?;\n(?=\n|const |export )/gm,
  ))
    if (!new RegExp(`^const ${name} =`, 'm').test(text)) {
      const at = text.indexOf('\nexport const feature');
      text = `${text.slice(0, at)}\n${statement.trimEnd()}\n${text.slice(at)}`;
    }
  // Its slots: entries join a list the feature fills already, and a new key goes last.
  for (const key of entries(piece, manifest(piece)).entries) {
    if (key.name === 'name') continue;
    const { entries: own, end } = entries(text, manifest(text));
    const have = own.find((each) => each.name === key.name);
    if (!have)
      text = `${text.slice(0, end).trimEnd()}\n${key.text.replace(/^\n+/, '').trim()}\n${text.slice(end)}`;
    else if (key.text.includes('[') && text.slice(have.start, have.end).includes('[')) {
      // An entry's id is its own among the feature's: one there already is the feature's.
      const ids = (list: string) =>
        items(list.slice(list.indexOf('['))).map((item) => /^\{\s*id: '([^']+)'/.exec(item)?.[1]);
      const taken = ids(text.slice(have.start, have.end));
      const twice = ids(key.text).find((id) => id && taken.includes(id));
      if (twice) throw new UsageError(`Its ${key.name} holds ${twice} already`);
      text = appended(
        text,
        text.indexOf('[', have.start),
        items(key.text.slice(key.text.indexOf('['))),
      );
    }
  }
  return text;
}

/** Rust source as rustfmt leaves it, read from its input so it follows no `mod`. */
export function rustfmt(source: string) {
  const formatted = spawnSync('rustfmt', ['--edition', '2024', '--emit', 'stdout', '-q'], {
    cwd: repositoryRoot,
    input: source,
    encoding: 'utf8',
  });
  if (formatted.status !== 0)
    throw new Error(`The merged Rust does not parse:\n${formatted.stderr}`);
  return formatted.stdout;
}

// ---- Cargo.toml and the rest

/** Where the manifest's `[section]` is: from its header to the next one, if it has one. */
function tomlSection(text: string, section: string) {
  const found = new RegExp(`(^|\\n)\\[${section}\\]\\n`).exec(text);
  if (!found) return undefined;
  const next = text.indexOf('\n[', found.index + 1);
  return { start: found.index, end: next < 0 ? text.length : next, last: next < 0 };
}
/** The manifest with each `key = value` the piece's lists that it lacks, in its section. */
export function mergeToml(target: string, piece: string) {
  let text = target;
  let section = '';
  for (const line of piece.split('\n')) {
    const header = /^\[([^\]]+)\]$/.exec(line);
    if (header) {
      section = header[1]!;
      continue;
    }
    const key = /^([\w-]+) = /.exec(line)?.[1];
    if (!key || !['dependencies', 'dev-dependencies', 'features'].includes(section)) continue;
    const lists = (name: string) => {
      const at = tomlSection(text, name);
      return Boolean(at) && new RegExp(`^${key} = `, 'm').test(text.slice(at!.start, at!.end));
    };
    // A test's dependency the build lists already is the build's, unless it asks for more of it.
    if (section === 'dev-dependencies' && !line.includes('features') && lists('dependencies'))
      continue;
    const at = tomlSection(text, section);
    if (!at) {
      text = `${text.trimEnd()}\n\n[${section}]\n${line}\n`;
      continue;
    }
    if (lists(section)) continue;
    text = `${text.slice(0, at.end).trimEnd()}\n${line}\n${text.slice(at.end).replace(/^\n*/, at.last ? '' : '\n')}`;
  }
  return text;
}

/**
 * Has every test of the owner in `dir` that installs it with core's test registry install the
 * collector or feature it now depends on too: `<helper>::install(&[collectors], &[features])`,
 * with or without the crate's prefix.
 */
export function installing(
  root: string,
  plan: Plan,
  dir: string,
  dependency: { collector?: string; feature?: string },
  only?: string,
) {
  const [name, list] = dependency.collector ? [dependency.collector, 0] : [dependency.feature!, 1];
  const item = `&dispatch_${name}::${list === 0 ? 'COLLECTOR' : 'FEATURE'}`;
  const there = new RegExp(`\\b(dispatch_)?${name}::${list === 0 ? 'COLLECTOR' : 'FEATURE'}\\b`);
  const written = [...plan.files.keys()].filter(
    (file) => file.startsWith(`${dir}/`) && file.endsWith('.rs'),
  );
  for (const file of [...files(root, dir, /\.rs$/), ...written]) {
    if (only && file !== only) continue;
    const fresh = plan.files.has(file);
    const current = fresh ? plan.files.get(file)! : (plan.changes.get(file) ?? read(root, file));
    const merged = current.replace(
      /\b(\w+)::install\(\s*&\[([^\]]*)\],\s*&\[([^\]]*)\]/g,
      (call, helper: string, collectors: string, features: string) => {
        const lists = [collectors, features];
        if (there.test(lists[list]!)) return call;
        lists[list] = [...items(`[${lists[list]}]`), item].join(', ');
        return `${helper}::install(&[${lists[0]}], &[${lists[1]}]`;
      },
    );
    if (merged !== current) (fresh ? plan.files : plan.changes).set(file, merged);
  }
}

export async function planAdd(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  const [piece, value, ...extra] = args.positional;
  const name = args.options.get('to');
  const known = piece ? PIECES[piece] : undefined;
  if (!known || extra.length || Boolean(value) !== known.valued)
    throw new UsageError(`Name one piece: ${Object.keys(PIECES).join(', ')}, with its value`);
  if (!name) throw new UsageError('Name the feature to add it to: --to <feature>');
  const shown = args.flags.has('shown');
  if (shown && piece !== 'contract')
    throw new UsageError('--shown is what a page shows of a contract: add contract <name> --shown');
  if (!features(root).includes(name))
    throw new UsageError(`--to names a feature, and ${name} is none`);
  const dir = `features/${name}`;
  if (piece === 'api' && exists(root, `${dir}/api`))
    throw new UsageError(`${name} has api/ already: add its endpoints there`);
  if (piece === 'page' && pageOf(root, name))
    throw new UsageError(`${name} has a page already: add a tab of it with add tab-of ${name}`);
  if (piece === 'people' && !args.options.has('table'))
    throw new UsageError('people reads who a table names: add --table <table>');
  if (piece === 'contract' && exists(root, `${dir}/contract`))
    throw new UsageError(`${name} has contract/ already: add to the contract it has`);
  if (piece === 'settings' && exists(root, `${dir}/backend/settings.rs`))
    throw new UsageError(
      `${name} declares its settings in backend/settings.rs already: add a section or a field there`,
    );
  const manifest = read(root, `${dir}/feature.rs`);
  if (shown && !new RegExp(`\\bperm\\(\\s*"${name}\\.view"`).test(manifest))
    throw new UsageError(
      `A page shows ${value} to those holding ${name}.view, which ${name} lacks: add it, or show ${value} by hand`,
    );
  // A permission the piece uses that the feature retired can't come back.
  const retiredAt = manifest.indexOf('retired_identifiers: &[');
  const retired =
    retiredAt < 0 ? '' : manifest.slice(retiredAt, closing(manifest, retiredAt + 22) + 1);
  for (const use of known.uses) {
    const permission = `${name}.${use}`;
    if (retired.includes(`("${permission}"`))
      throw new UsageError(
        `${name} retired ${permission}, which the ${piece} would use: add it by hand`,
      );
  }
  if (piece === 'tab-of' && value === name)
    throw new UsageError(`A tab of ${name}'s own page is one of its own tabs: add it there`);
  // Its people's table, by the database the feature keeps it in.
  const tables =
    (
      JSON.parse(read(root, 'app/generated/features.json')) as Record<
        string,
        { tables: Record<string, string[]> }
      >
    )[name]?.tables ?? {};
  const table = args.options.get('table');
  const database = Object.entries(tables).find(([, list]) => table && list.includes(table))?.[0];
  if (
    piece === 'people' &&
    (!database || (database !== 'dsp' && !collectors(root).includes(database)))
  )
    throw new UsageError(
      database
        ? `${table} is in ${name}'s own database: connect its people by hand`
        : `--table names one of ${name}'s tables: ${Object.values(tables).flat().join(', ') || 'it has none'}`,
    );
  const label = /\b(?:optional|mandatory)\(\s*"[^"]+",\s*"([^"]+)"/.exec(manifest)?.[1];
  const { pascal, slug } = names(name);
  // The piece as the feature's templates write it, for the feature as it is.
  const { plan: rendered } = await renderFeature(
    root,
    [
      name,
      ...(label ? ['--label', label] : []),
      ...(/\bswitch:\s*mandatory\(/.test(manifest) ? ['--mandatory'] : []),
      `--${piece}`,
      ...(value ? [value] : []),
      ...(piece === 'people' ? ['--tables', database!, '--table', table!] : []),
      ...(args.options.has('icon') ? ['--icon', args.options.get('icon')!] : []),
      ...(shown ? ['--shown'] : []),
    ],
    true,
  );
  const plan: Plan = { files: new Map(), changes: new Map(), notes: [] };
  const left: string[] = [];
  // A new feature's own storage and first migration, which a piece added to one never needs.
  const own = /\/(backend\/storage\.rs|tests\/backend\/(storage|feature)\.rs|migrations\/.*)$/;
  // A browser test only for a piece with a screen of its own.
  const screen = ['page', 'tab-of'].includes(piece!);
  const wanted = [...rendered.files].filter(
    ([file]) => !own.test(file) && (screen || !file.includes('/tests/browser/')),
  );
  const wrote = (file: string) => exists(root, file) || wanted.some(([each]) => each === file);
  const fresh = new Set(wanted.map(([file]) => file).filter((file) => !exists(root, file)));
  // What only the piece writes, as its screen or its people, is new: one there is the
  // feature's own, which the piece won't wire in its place.
  const merges =
    /\/(feature\.rs|frontend\/feature\.ts|Cargo\.toml|mod\.rs|routes\.txt|README\.md|frontend\/index\.ts|frontend\/platform-slots\.ts|[^/]+\.spec\.ts)$/;
  const there = wanted
    .map(([file]) => file)
    .filter((file) => !fresh.has(file) && !merges.test(file));
  if (there.length)
    throw new UsageError(`${name} has ${there.join(', ')} already: the ${piece} would write it`);
  for (const [file, content] of wanted) {
    if (!exists(root, file)) {
      plan.files.set(file, content);
      continue;
    }
    const current = read(root, file);
    let merged: string;
    if (file === `${dir}/feature.rs`)
      merged = mergeRust(current, content, left, known.uses, (module) =>
        fresh.has(`${dir}/${module}.rs`),
      );
    else if (file === `${dir}/frontend/feature.ts`)
      merged = mergeTypeScript(current, content, pascal);
    else if (file.endsWith('/Cargo.toml')) merged = mergeToml(current, content);
    else if (/\/mod\.rs$/.test(file))
      // Its modules whose files are there or written now.
      merged = content
        .split('\n')
        .filter((line) => {
          const module = /mod (\w+);/.exec(line)?.[1];
          const at = `${file.slice(0, file.lastIndexOf('/'))}/${module}`;
          const declared = new RegExp(`^(pub(\\([^)]*\\))? )?mod ${module};`, 'm');
          return module && !declared.test(current) && (wrote(`${at}.rs`) || exists(root, at));
        })
        .reduce((text, line) => {
          // After its last module, not one under an attribute as its tests are, or at its end
          // when it has none.
          const last = [...text.matchAll(/^(?:pub(?:\([^)]*\))? )?mod \w+;$/gm)]
            .filter((found) => !/#\[[^\n]*\]\n$/.test(text.slice(0, found.index)))
            .at(-1);
          if (!last) return `${text.trimEnd()}\n${line}\n`;
          const at = last.index! + last[0].length;
          return `${text.slice(0, at)}\n${line}${text.slice(at)}`;
        }, current);
    else if (file.endsWith('/routes.txt'))
      merged = content
        .split('\n')
        .filter((line) => /^[A-Z]+ \//.test(line) && !current.includes(line))
        .reduce((text, line) => `${text.trimEnd()}\n${line}\n`, current);
    else if (file.endsWith('/README.md'))
      // Its line, after the README's last line of what the feature declares.
      merged = content
        .split('\n')
        .filter((line) => line.startsWith(`- **${known.readme}:**`) && !current.includes(line))
        .reduce((text, line) => {
          const last = [...text.matchAll(/^- \*\*[^*]+\*\*.*(?:\n {2,}\S.*)*$/gm)].at(-1);
          if (!last) return `${text.trimEnd()}\n\n${line}\n`;
          const at = last.index! + last[0].length;
          return `${text.slice(0, at)}\n${line}${text.slice(at)}`;
        }, current);
    else if (/\/frontend\/index\.ts$/.test(file))
      // What its page's code exports.
      merged = content
        .split('\n')
        .filter((line) => line.startsWith('export ') && !current.includes(line))
        .reduce((text, line) => `${text.trimEnd()}\n${line}\n`, current);
    else if (file.endsWith('/platform-slots.ts')) continue;
    else if (file.endsWith('.spec.ts'))
      merged = `${mergeImports(current, content).trimEnd()}\n${content.slice(content.search(/\ntest\(/)).trimEnd()}\n`;
    else continue;
    if (merged !== current) plan.changes.set(file, await format(file, merged));
  }
  // A collector its people come from is one its tests install it with, as the registry wants
  // each feature's dependencies installed.
  const site = peopleSources(root).find((source) => source.id === value)?.site;
  if (piece === 'people' && site) installing(root, plan, dir, { collector: site });
  if (shown) await showing(root, plan, value!);
  // The test a contract or settings brings installs the feature with what it uses: the
  // collectors and features the feature map says it uses, what its other tests install, however
  // they name it, and what its manifest, as the piece leaves it, depends on.
  const tested = ({ contract: 'exposed', settings: 'settings' } as Record<string, string>)[piece!];
  if (tested) {
    const now = (file: string) => plan.changes.get(file) ?? read(root, file);
    const used = new Map<string, 'collector' | 'feature'>();
    const kindOf = (each: string) =>
      exists(root, `collectors/${each}/collector.rs`)
        ? 'collector'
        : each !== name && exists(root, `features/${each}/feature.rs`)
          ? 'feature'
          : undefined;
    const mapped = (
      JSON.parse(read(root, 'app/generated/features.json')) as Record<
        string,
        { collectors: string[]; depends_on: string[] }
      >
    )[name];
    for (const each of [...(mapped?.collectors ?? []), ...(mapped?.depends_on ?? [])]) {
      const kind = kindOf(each);
      if (kind) used.set(each, kind);
    }
    for (const file of files(root, dir, /\.rs$/))
      for (const [, collectors, features] of now(file).matchAll(
        /::install\(\s*&\[([^\]]*)\],\s*&\[([^\]]*)\]/g,
      ))
        for (const [, each] of `${collectors} ${features}`.matchAll(
          /&(?:dispatch_)?(\w+)::(?:COLLECTOR|FEATURE)\b/g,
        )) {
          const kind = kindOf(each!);
          if (kind) used.set(each!, kind);
        }
    const declared = /\bdepends_on:\s*&\[([^\]]*)\]/.exec(now(`${dir}/feature.rs`))?.[1] ?? '';
    for (const [, each] of declared.matchAll(/"([^"]+)"/g)) {
      const kind = kindOf(each!);
      if (kind) used.set(each!, kind);
    }
    for (const [each, kind] of used)
      installing(root, plan, dir, { [kind]: each }, `${dir}/tests/backend/${tested}.rs`);
  }
  if (piece === 'settings') plan.notes.push(settingsNote(name, slug));
  // A test a generator stamped that only restates a manifest alone, with no permission, goes
  // once the piece gives it one, with its module; and so does the empty backend/ a manifest
  // alone keeps, which a piece with an API or a screen doesn't need. A test or a module the
  // author wrote into stays: only the stamped assertion goes from a test of theirs.
  const stamped = `${dir}/tests/backend/feature.rs`;
  const declared = '#[cfg(test)]\n#[path = "tests/backend/feature.rs"]\nmod tests;\n';
  const assertion = /^\s*assert!\(FEATURE\.permissions\.is_empty\(\)\);\n/m;
  const rust = plan.changes.get(`${dir}/feature.rs`);
  const test = exists(root, stamped) ? read(root, stamped) : '';
  if (rust && assertion.test(test) && !/\bperm\(/.test(manifest) && /\bperm\(/.test(rust)) {
    const alone = test.match(/#\[test\]/g)?.length === 1;
    if (!alone || !rust.includes(declared)) {
      plan.changes.set(stamped, test.replace(assertion, ''));
      plan.notes.push(`${stamped} no longer asserts the feature has no permissions.`);
    } else {
      let grown = rust.replace(`\n${declared}`, '').replace(declared, '');
      const removed = [stamped];
      const backend = files(root, `${dir}/backend`, /./);
      const writesBackend = [...plan.files.keys()].some((file) =>
        file.startsWith(`${dir}/backend/`),
      );
      // Only comments and blank lines: the module the generator stamped, holding nothing.
      const empty = (file: string) =>
        read(root, file)
          .split('\n')
          .every((line) => !line.trim() || line.trim().startsWith('//'));
      if (backend.length === 1 && !writesBackend && empty(backend[0]!)) {
        removed.push(backend[0]!);
        grown = grown.replace(/^mod backend;\n/m, '');
      }
      plan.changes.set(`${dir}/feature.rs`, grown);
      plan.removed = removed;
      plan.notes.push(`${stamped} restated the manifest it had alone: write its first real test.`);
    }
  }
  // Its Rust as rustfmt leaves it: the files it writes together, and each it changes alone,
  // since that file's modules aren't beside it.
  formatRust(plan);
  for (const [file, content] of plan.changes)
    if (file.endsWith('.rs')) plan.changes.set(file, rustfmt(content));
  plan.notes.push(...left.map((note) => `By hand: ${note}.`));
  plan.notes.push('`dispatchdev generate` writes what follows from it.');
  return { plan, args };
}

export const generator: Generator = {
  kind: 'piece',
  about: 'a piece added to a feature that exists',
  usage,
  flags: FLAGS,
  options: OPTIONS,
  plan: planAdd,
  generates: true,
};
