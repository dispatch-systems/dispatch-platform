import { spawnSync } from 'node:child_process';
import { UsageError, repositoryRoot } from './common.js';

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
export function closing(text: string, at: number) {
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
/** The keys of the DSP route with the id in a frontend manifest's routes, if it declares it. */
export function routeKeys(text: string, id: string) {
  const routes = text.indexOf('routes: [');
  const at =
    routes < 0 ? -1 : text.slice(routes).search(new RegExp(`\\bid: '${id}',\\s*scope: 'dsp'`));
  return at < 0 ? undefined : entries(text, text.lastIndexOf('{', routes + at)).entries;
}
/** The items of a list's `[...]` text, at its top level. */
export function items(list: string) {
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
export function mergeImports(text: string, piece: string) {
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
      // A value's import among the module's, before its types', and React's first of all, as
      // the manifests have it.
      const present = imports(text);
      if (wanted.from === 'react' && present[0]) {
        const at = text.indexOf(present[0].whole);
        text = `${text.slice(0, at)}${wanted.whole}\n${text.slice(at)}`;
      } else {
        const last = present.filter((each) => wanted.type || !each.type).at(-1) ?? present.at(-1);
        const at = last ? text.indexOf(last.whole) + last.whole.length : 0;
        text = `${text.slice(0, at)}\n${wanted.whole}${text.slice(at)}`;
      }
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
  // Its constants, together before the manifest.
  const constants = [...piece.matchAll(/^const (\w+) =[\s\S]*?;\n(?=\n|const |export )/gm)]
    .filter(([, name]) => !new RegExp(`^const ${name} =`, 'm').test(text))
    .map(([statement]) => statement.trimEnd());
  if (constants.length) {
    const at = text.indexOf('\nexport const feature');
    text = `${text.slice(0, at)}\n${constants.join('\n')}\n${text.slice(at)}`;
  }
  // Its slots: entries join a list the feature fills already, and a new key goes where the piece
  // has it, before the first key after it there that the manifest has, or else last.
  const keys = entries(piece, manifest(piece)).entries;
  for (const [index, key] of keys.entries()) {
    if (key.name === 'name') continue;
    const { entries: own, end } = entries(text, manifest(text));
    const have = own.find((each) => each.name === key.name);
    const next = keys
      .slice(index + 1)
      .map((later) => own.find((each) => each.name === later.name))
      .find(Boolean);
    if (!have && next)
      text = `${text.slice(0, next.start)}\n${key.text.trim()}${text.slice(next.start)}`;
    else if (!have)
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
