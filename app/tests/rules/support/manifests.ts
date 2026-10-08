import { closing, includes, lexFile, literalAt, rust, type Lexed } from './rust.js';
import { collectors, features, files, isFile, isTestFile, ownerOf, type Owner } from './repo.js';

/** A span of a file's source, as offsets into it. */
export type Span = { file: string; start: number; end: number };
const sourceOf = (span: Span): Lexed => lexFile(span.file);
export const textOf = (span: Span) => sourceOf(span).masked.slice(span.start, span.end).trim();

/** Rust the product compiles: an owner's code outside its tests. */
export const productRust = (file: string) =>
  file.endsWith('.rs') && !isTestFile(file) && !!ownerOf(file) && rust().mounts.has(file);
/** Whether `offset` sits in code compiled only for tests. */
export const inTest = (file: string, offset: number) =>
  isTestFile(file) ||
  rust()
    .modulesAt(file, offset)
    .every((module) => module.test);

/** The fields of the struct literal whose `{` is at `open`, each with its value's span. */
export function fieldsAt(file: string, open: number): Map<string, Span> {
  const { masked } = lexFile(file);
  const close = closing(masked, open);
  const found = new Map<string, Span>();
  let at = open + 1;
  while (at < close) {
    const head = /^\s*(\.\.|[A-Za-z_][A-Za-z0-9_]*)\s*(:(?!:))?/.exec(masked.slice(at, close));
    if (!head) break;
    const start = at + head[0].length;
    let end = start;
    const stack: string[] = [];
    for (; end < close; end++) {
      const char = masked[end]!;
      if ('([{'.includes(char)) stack.push(char);
      else if (')]}'.includes(char)) stack.pop();
      else if (char === ',' && !stack.length) break;
    }
    found.set(head[1]!, { file, start: head[2] || head[1] === '..' ? start : at, end });
    at = end + 1;
  }
  return found;
}

const literalScans = new Map<
  string,
  { file: string; offset: number; fields: Map<string, Span> }[]
>();
/**
 * Every struct literal of `type` in the product's Rust, by its fields. A function body that
 * returns the type, an `impl` and the type's own declaration are not literals.
 */
export function literals(type: string, scope: (file: string) => boolean = () => true) {
  let found = literalScans.get(type);
  if (!found) {
    found = [];
    const pattern = new RegExp(`(?<![A-Za-z0-9_])${type}\\s*\\{`, 'g');
    for (const file of files.filter(productRust)) {
      const { masked } = lexFile(file);
      for (const match of masked.matchAll(pattern)) {
        const before = masked.slice(Math.max(0, match.index - 40), match.index);
        if (/\b(struct|enum|union|impl|for|trait|type)\s+$|->\s*$/.test(before)) continue;
        const open = match.index + match[0].length - 1;
        const head = masked.slice(open + 1, open + 80);
        if (!/^\s*(\}|\.\.|[A-Za-z_][A-Za-z0-9_]*\s*:(?!:))/.test(head)) continue;
        if (inTest(file, match.index)) continue;
        found.push({ file, offset: match.index, fields: fieldsAt(file, open) });
      }
    }
    literalScans.set(type, found);
  }
  return found.filter(({ file }) => scope(file));
}

/** Calls of `name(…)` in the product's Rust, by the span of their arguments. */
export function calls(name: string, scope: (file: string) => boolean = () => true): Span[] {
  const pattern = new RegExp(`(?<![A-Za-z0-9_:.])${name}\\s*\\(`, 'g');
  return files
    .filter((file) => productRust(file) && scope(file))
    .flatMap((file) => {
      const { masked } = lexFile(file);
      return [...masked.matchAll(pattern)]
        .filter(
          (match) => !/\bfn\s+$/.test(masked.slice(Math.max(0, match.index - 10), match.index)),
        )
        .filter((match) => !inTest(file, match.index))
        .map((match) => {
          const open = match.index + match[0].length - 1;
          return { file, start: open + 1, end: closing(masked, open) };
        });
    });
}

/** The span of a constant's initializer, wherever `name` is declared as one in `file`. */
function constant(file: string, name: string): Span | undefined {
  const { masked } = lexFile(file);
  const match = new RegExp(`\\b(?:const|static)\\s+${name}\\s*:[^=;]*=`).exec(masked);
  if (!match) return undefined;
  const start = match.index + match[0].length;
  let end = start;
  const stack: string[] = [];
  for (; end < masked.length; end++) {
    const char = masked[end]!;
    if ('([{'.includes(char)) stack.push(char);
    else if (')]}'.includes(char)) stack.pop();
    else if (char === ';' && !stack.length) break;
  }
  return { file, start, end };
}

/**
 * What an expression stands for once its paths are followed to the constants they name:
 * the span of the innermost expression that is not a path.
 */
export function follow(span: Span, depth = 0): Span {
  const text = textOf(span);
  if (depth > 8 || !/^[A-Za-z_][A-Za-z0-9_]*(\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*$/.test(text))
    return span;
  const segments = text.split('::').map((segment) => segment.trim());
  const last = segments.at(-1)!;
  const local = constant(span.file, last);
  if (segments.length === 1 && local) return follow(local, depth + 1);
  // Through whichever crate mounts the file and can name the constant.
  for (const module of rust().modulesAt(span.file, span.start)) {
    const target = rust().resolve(module, segments);
    const found = target && constant(target.file, last);
    if (found) return follow(found, depth + 1);
  }
  return span;
}
/** The string an expression stands for: a literal, or a constant set to one. */
export function stringOf(span: Span): string | undefined {
  const target = follow(span);
  const source = sourceOf(target);
  const literal = literalAt(source, target.start);
  return literal && /^\s*$/.test(source.masked.slice(literal.end, target.end))
    ? literal.value
    : undefined;
}
/** The first string literal inside what an expression stands for, as in `Kind::new("dsp", 1)`. */
export function firstString(span: Span): string | undefined {
  const target = follow(span);
  return sourceOf(target).literals.find((l) => l.start >= target.start && l.end <= target.end)
    ?.value;
}
/** The string literals inside a span, such as a list of ids. */
export const stringsIn = (span: Span) =>
  sourceOf(span)
    .literals.filter((l) => l.start >= span.start && l.end <= span.end)
    .map((l) => l.value);

/** A feature's manifest, as `features/<name>/feature.rs` declares it. */
export type FeatureManifest = {
  owner: Owner;
  file: string;
  name?: string;
  fields: Map<string, Span>;
  switch?: string;
  /** Whether every DSP has it, `switch: mandatory(..)`, rather than switching it. */
  mandatory: boolean;
  /** Its page's tabs, among its sub-features. */
  tabs: string[];
  /** Its page's other sub-features. */
  subs: string[];
  /** Its own permissions and its sub-features'. */
  permissions: string[];
  /** The features and collectors it declares it depends on. */
  dependsOn: string[];
};
const featureScans = new Map<string, FeatureManifest | undefined>();
export function featureManifest(owner: Owner): FeatureManifest | undefined {
  if (!featureScans.has(owner.dir)) featureScans.set(owner.dir, readFeature(owner));
  return featureScans.get(owner.dir);
}
function readFeature(owner: Owner): FeatureManifest | undefined {
  const file = `${owner.dir}/feature.rs`;
  if (!isFile(file)) return undefined;
  const { masked } = lexFile(file);
  const literal = /(?<![A-Za-z0-9_])Feature\s*\{/.exec(masked);
  const fields = literal ? fieldsAt(file, literal.index + literal[0].length - 1) : new Map();
  const named = fields.get('name');
  const base = /(?<![A-Za-z0-9_])feature\s*\(/.exec(masked);
  const name = named
    ? stringOf(named)
    : base
      ? literalAt(lexFile(file), base.index + base[0].length)?.value
      : undefined;
  const value = (field: string) => fields.get(field) as Span | undefined;
  const switchField = value('switch');
  const switchSpan =
    switchField && !/^None$/.test(textOf(switchField)) ? follow(switchField) : undefined;
  const switchId =
    switchSpan &&
    (() => {
      const id = /\bid\s*:/.exec(
        sourceOf(switchSpan).masked.slice(switchSpan.start, switchSpan.end),
      );
      return id
        ? literalAt(sourceOf(switchSpan), switchSpan.start + id.index + id[0].length)?.value
        : firstString(switchSpan);
    })();
  const callsIn = (span: Span | undefined, callee: string) => {
    if (!span) return [];
    const target = follow(span);
    const source = sourceOf(target);
    return [
      ...source.masked
        .slice(target.start, target.end)
        .matchAll(new RegExp(`\\b${callee}\\s*\\(`, 'g')),
    ]
      .map((match) => literalAt(source, target.start + match.index + match[0].length)?.value)
      .filter((id): id is string => !!id);
  };
  const permissions = value('permissions');
  const mandatory =
    !!switchSpan &&
    /^\s*mandatory\s*\(/.test(sourceOf(switchSpan).masked.slice(switchSpan.start, switchSpan.end));
  return {
    owner,
    file,
    ...(name ? { name } : {}),
    fields,
    ...(switchId ? { switch: switchId } : {}),
    mandatory,
    tabs: callsIn(value('subfeatures'), 'tab'),
    subs: callsIn(value('subfeatures'), 'sub'),
    permissions: [
      ...callsIn(permissions, 'perm'),
      ...callsIn(value('subfeatures'), 'perm'),
      ...(permissions ? permissionLiterals(follow(permissions)) : []),
    ],
    dependsOn: value('depends_on') ? stringsIn(follow(value('depends_on')!)) : [],
  };
}
function permissionLiterals(span: Span): string[] {
  const { masked } = sourceOf(span);
  return [...masked.slice(span.start, span.end).matchAll(/\bPermission\s*\{/g)].flatMap((match) => {
    const id = fieldsAt(span.file, span.start + match.index + match[0].length - 1).get('id');
    const value = id && stringOf(id);
    return value ? [value] : [];
  });
}
export const featureManifests = () =>
  features().flatMap((owner) => {
    const manifest = featureManifest(owner);
    return manifest ? [manifest] : [];
  });

/** A collector's manifest, `collectors/<site>/collector.rs`, and the id it declares. */
export type CollectorManifest = { owner: Owner; file: string; id?: string };
export function collectorManifest(owner: Owner): CollectorManifest | undefined {
  const file = `${owner.dir}/collector.rs`;
  if (!isFile(file)) return undefined;
  const source = lexFile(file);
  // Its id: the provider it declares, or what its `id` answers.
  const provider =
    /\bProvider\s*::\s*new\s*\(/.exec(source.masked) ??
    /\bfn\s+id\s*\(\s*&\s*self\s*\)[^{]*\{\s*/.exec(source.masked);
  const id = provider && literalAt(source, provider.index + provider[0].length)?.value;
  return { owner, file, ...(id ? { id } : {}) };
}

/** One collection a collector offers, by its job kind. */
export type Collection = { owner: Owner; jobKind: string; file: string; offset: number };
let collected: Collection[] | undefined;
export const collections = (): Collection[] =>
  (collected ??= literals('Collection', (file) => ownerOf(file)?.layer === 'collector').flatMap(
    ({ file, offset, fields }) => {
      const kind = fields.get('job_kind');
      const jobKind = kind && stringOf(kind);
      return jobKind ? [{ owner: ownerOf(file)!, jobKind, file, offset }] : [];
    },
  ));
/** One keeper: an `impl Keeper for …`, with the job kind its `keeps` answers. */
export type Keeper = { owner: Owner; type: string; jobKind?: string; file: string; offset: number };
let keeperScan: Keeper[] | undefined;
export function keepers(): Keeper[] {
  return (keeperScan ??= files
    .filter((file) => productRust(file))
    .flatMap((file) => {
      const source = lexFile(file);
      return [
        ...source.masked.matchAll(
          /\bimpl\s+(?:[A-Za-z_:]*::)?Keeper\s+for\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/g,
        ),
      ]
        .filter((match) => !inTest(file, match.index))
        .map((match) => {
          const open = match.index + match[0].length - 1;
          const body = source.masked.slice(open, closing(source.masked, open));
          const keeps = /\bfn\s+keeps\s*\([^)]*\)[^{]*\{/.exec(body);
          let jobKind: string | undefined;
          if (keeps) {
            const start = open + keeps.index + keeps[0].length;
            jobKind = stringOf({ file, start, end: closing(source.masked, start - 1) });
          }
          return {
            owner: ownerOf(file)!,
            type: match[1]!,
            ...(jobKind ? { jobKind } : {}),
            file,
            offset: match.index,
          };
        });
    }));
}

/** One migration, as an owner's `Migrations` list declares it, or a feature's `OwnMigrations`. */
export type Migration = {
  owner: Owner;
  /** Numbered by its feature from 1 and recorded under its name, rather than in the
   * database's one list. */
  owned: boolean;
  database?: string;
  id: number;
  name?: string;
  /** The SQL file it applies, or the SQL text written in place. */
  sql?: { file?: string; text: string };
  /** The function a `Code` migration runs. */
  code?: string;
  /** The SQL files a `Code` migration's function includes. */
  includes: string[];
  file: string;
  offset: number;
};
export function migrations(type: 'Migrations' | 'OwnMigrations' = 'Migrations'): Migration[] {
  return literals(type).flatMap(({ file, fields }) => {
    const owner = ownerOf(file)!;
    const kind = fields.get('kind');
    const database = kind && firstString(kind);
    const list = fields.get('list');
    if (!list) return [];
    const target = follow(list);
    const source = sourceOf(target);
    return [...source.masked.slice(target.start, target.end).matchAll(/\bMigration\s*\{/g)].map(
      (match) => {
        const offset = target.start + match.index;
        const migration = fieldsAt(target.file, offset + match[0].length - 1);
        const id = Number(textOf(migration.get('id')!));
        const name = migration.get('name') && stringOf(migration.get('name')!);
        const apply = migration.get('apply');
        const applied = apply ? textOf(apply) : '';
        const result: Migration = {
          owner,
          owned: type === 'OwnMigrations',
          ...(database ? { database } : {}),
          id,
          ...(name ? { name } : {}),
          includes: [],
          file: target.file,
          offset,
        };
        const sql = /(?:^|::)Sql\s*\(/.exec(applied);
        const code = /(?:^|::)Code\s*\(\s*([A-Za-z_][A-Za-z0-9_:]*)\s*\)/.exec(applied);
        if (sql && apply) {
          const included = includes(target.file).find(
            (include) => include.offset >= apply.start && include.offset < apply.end,
          );
          if (included)
            result.sql = {
              file: included.target,
              text: isFile(included.target) ? lexFile(included.target).text : '',
            };
          else {
            const literal = sourceOf(apply).literals.find(
              (l) => l.start >= apply.start && l.end <= apply.end,
            );
            if (literal) result.sql = { text: literal.value };
          }
        } else if (code && apply) {
          const path = code[1]!.split('::').map((segment) => segment.trim());
          result.code = path.at(-1)!;
          result.includes = codeIncludes(target.file, apply.start, path);
        }
        return result;
      },
    );
  });
}
// The SQL files a migration function includes, found where the function is defined.
function codeIncludes(file: string, offset: number, path: string[]): string[] {
  const defining =
    (path.length > 1 &&
      rust()
        .modulesAt(file, offset)
        .map((module) => rust().resolve(module, path))
        .find(Boolean)?.file) ||
    file;
  const { masked } = lexFile(defining);
  const definition = new RegExp(`\\bfn\\s+${path.at(-1)}\\s*\\(`).exec(masked);
  if (!definition) return [];
  const open = masked.indexOf('{', definition.index);
  const close = closing(masked, open);
  return includes(defining)
    .filter((include) => include.offset > open && include.offset < close)
    .map((include) => include.target);
}

export const collectorManifests = () =>
  collectors().flatMap((owner) => {
    const manifest = collectorManifest(owner);
    return manifest ? [manifest] : [];
  });

/** The collections a feature's manifest keeps, by job kind, whichever way it names them. */
export function kept(owner: Owner): { jobKind?: string; at: string }[] {
  const manifest = featureManifest(owner);
  const keeps = manifest?.fields.get('keeps');
  if (!manifest || !keeps) return [];
  const span = follow(keeps);
  const { masked } = lexFile(span.file);
  const found: { jobKind?: string; at: string }[] = [];
  // `keep(cortex::MEALS, …)`: the collection, then how its data is kept.
  for (const match of masked.slice(span.start, span.end).matchAll(/\bkeep\s*\(/g)) {
    const open = span.start + match.index + match[0].length - 1;
    const argument = masked.slice(open + 1, closing(masked, open)).split(',')[0]!;
    const target = follow({ file: span.file, start: open + 1, end: open + 1 + argument.length });
    const literal = /^Collection\s*\{/.exec(textOf(target));
    const jobKind = literal
      ? (() => {
          const offset = lexFile(target.file).masked.indexOf('{', target.start);
          const field = fieldsAt(target.file, offset).get('job_kind');
          return field && stringOf(field);
        })()
      : stringOf(target);
    found.push({ ...(jobKind ? { jobKind } : {}), at: `${owner.dir}: keep(${argument.trim()})` });
  }
  // `&[&keeper::Dvic]`: a type implementing `Keeper`, listed by name.
  for (const keeper of keepers().filter((keeper) => keeper.owner.dir === owner.dir))
    if (new RegExp(`\\b${keeper.type}\\b`).test(textOf(span)))
      found.push({
        ...(keeper.jobKind ? { jobKind: keeper.jobKind } : {}),
        at: `${owner.dir}: ${keeper.type}`,
      });
  return found;
}

/**
 * The features and collectors a feature declares it uses: those its manifest's `depends_on`
 * names, and the collectors whose collections it keeps. Core is everyone's.
 */
const declarations = new Map<string, Set<string>>();
export function declared(owner: Owner): Set<string> {
  if (!declarations.has(owner.dir)) declarations.set(owner.dir, readDeclared(owner));
  return declarations.get(owner.dir)!;
}
function readDeclared(owner: Owner): Set<string> {
  if (owner.layer !== 'feature') return new Set();
  const names = featureManifest(owner)?.dependsOn ?? [];
  const dirs = names.map((name) =>
    isFile(`features/${name}/feature.rs`) || !isFile(`collectors/${name}/collector.rs`)
      ? `features/${name}`
      : `collectors/${name}`,
  );
  const kinds = new Set(kept(owner).flatMap(({ jobKind }) => (jobKind ? [jobKind] : [])));
  for (const collection of collections())
    if (kinds.has(collection.jobKind)) dirs.push(collection.owner.dir);
  return new Set(dirs);
}
