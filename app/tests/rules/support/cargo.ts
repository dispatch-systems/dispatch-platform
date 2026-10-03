import path from 'node:path';
import { exists, files, join, read } from './repo.js';

type Value = string | number | boolean | Value[] | { [key: string]: Value };
type Table = { [key: string]: Value };

/**
 * Enough TOML for Cargo manifests: tables, arrays of tables, dotted keys, strings, numbers,
 * booleans, arrays and inline tables, each possibly over several lines.
 */
export function parseToml(text: string): Table {
  const top: Table = {};
  let current = top;
  let at = 0;
  const fail = (message: string): never => {
    throw new Error(`${message} at offset ${at}: ${JSON.stringify(text.slice(at, at + 40))}`);
  };
  const space = (newlines: boolean) => {
    for (;;) {
      const char = text[at];
      if (char === ' ' || char === '\t' || char === '\r' || (newlines && char === '\n')) at++;
      else if (char === '#') while (at < text.length && text[at] !== '\n') at++;
      else return;
    }
  };
  const key = (): string[] => {
    const parts: string[] = [];
    for (;;) {
      space(false);
      if (text[at] === '"' || text[at] === "'") parts.push(string());
      else {
        const match = /^[A-Za-z0-9_-]+/.exec(text.slice(at));
        if (!match) fail('expected a key');
        parts.push(match![0]);
        at += match![0].length;
      }
      space(false);
      if (text[at] !== '.') return parts;
      at++;
    }
  };
  const string = (): string => {
    const quote = text[at]!;
    const multi = text.startsWith(quote.repeat(3), at);
    const close = multi ? quote.repeat(3) : quote;
    at += close.length;
    if (multi && text[at] === '\n') at++;
    let value = '';
    while (!text.startsWith(close, at)) {
      if (at >= text.length) fail('unterminated string');
      const char = text[at++]!;
      if (char === '\\' && quote === '"') {
        const escaped = text[at++]!;
        value += { n: '\n', t: '\t', r: '\r', '"': '"', '\\': '\\' }[escaped] ?? escaped;
      } else value += char;
    }
    at += close.length;
    return value;
  };
  const value = (): Value => {
    space(false);
    const char = text[at];
    if (char === '"' || char === "'") return string();
    if (char === '[') {
      at++;
      const list: Value[] = [];
      for (;;) {
        space(true);
        if (text[at] === ']') {
          at++;
          return list;
        }
        list.push(value());
        space(true);
        if (text[at] === ',') at++;
      }
    }
    if (char === '{') {
      at++;
      const table: Table = {};
      for (;;) {
        space(false);
        if (text[at] === '}') {
          at++;
          return table;
        }
        assign(table, key(), (at++, value()));
        space(false);
        if (text[at] === ',') at++;
      }
    }
    const match = /^[^\s,\]}#]+/.exec(text.slice(at));
    if (!match) fail('expected a value');
    at += match![0].length;
    if (match![0] === 'true' || match![0] === 'false') return match![0] === 'true';
    return Number(match![0].replaceAll('_', ''));
  };
  const assign = (table: Table, keys: string[], item: Value) => {
    let target = table;
    for (const part of keys.slice(0, -1)) target = (target[part] ??= {}) as Table;
    target[keys.at(-1)!] = item;
  };
  while (at < text.length) {
    space(true);
    if (at >= text.length) break;
    if (text.startsWith('[[', at)) {
      at += 2;
      const keys = key();
      at += 2;
      let parent = top;
      for (const part of keys.slice(0, -1)) parent = (parent[part] ??= {}) as Table;
      const list = (parent[keys.at(-1)!] ??= []) as Table[];
      list.push((current = {}));
    } else if (text[at] === '[') {
      at++;
      const keys = key();
      at++;
      current = top;
      for (const part of keys) current = (current[part] ??= {}) as Table;
    } else {
      const keys = key();
      if (text[at] !== '=') fail('expected =');
      at++;
      assign(current, keys, value());
    }
  }
  return top;
}

export type Target = { kind: 'lib' | 'bin' | 'test' | 'example'; name: string; file: string };
export type Dependency = { name: string; dev: boolean; path?: string };
/** A crate of the workspace, as its Cargo.toml declares it. */
export type Crate = {
  manifest: string;
  dir: string;
  name: string;
  /** Its name in Rust paths. */
  ident: string;
  targets: Target[];
  dependencies: Dependency[];
};

const crateIdent = (name: string) => name.replaceAll('-', '_');
function dependencies(table: Table | undefined, dev: boolean, dir: string): Dependency[] {
  return Object.entries(table ?? {}).map(([name, spec]) => {
    const local =
      typeof spec === 'object' && !Array.isArray(spec) && typeof spec.path === 'string'
        ? join(`${dir}/Cargo.toml`, spec.path)
        : undefined;
    const renamed =
      typeof spec === 'object' && !Array.isArray(spec) && typeof spec.package === 'string'
        ? spec.package
        : undefined;
    return { name: renamed ?? name, dev, ...(local ? { path: local } : {}) };
  });
}
export function readCrate(manifest: string): Crate {
  const toml = parseToml(read(manifest));
  const dir = path.posix.dirname(manifest);
  const pkg = toml.package as Table;
  const name = pkg.name as string;
  const at = (file: string) => join(manifest, file);
  const targets: Target[] = [];
  const lib = toml.lib as Table | undefined;
  const libFile = at((lib?.path as string | undefined) ?? 'src/lib.rs');
  if (lib || exists(libFile)) targets.push({ kind: 'lib', name: crateIdent(name), file: libFile });
  const listed = (kind: 'bin' | 'test' | 'example') =>
    ((toml[kind] as Table[] | undefined) ?? []).map((target) => ({
      kind,
      name: crateIdent((target.name as string | undefined) ?? name),
      file: at(target.path as string),
    }));
  targets.push(...listed('bin'), ...listed('test'), ...listed('example'));
  if (!toml.bin && exists(at('src/main.rs')))
    targets.push({ kind: 'bin', name: crateIdent(name), file: at('src/main.rs') });
  const tables = [toml, ...Object.values((toml.target as Table | undefined) ?? {})] as Table[];
  return {
    manifest,
    dir,
    name,
    ident: crateIdent(name),
    targets,
    dependencies: tables.flatMap((table) => [
      ...dependencies(table.dependencies as Table | undefined, false, dir),
      ...dependencies(table['dev-dependencies'] as Table | undefined, true, dir),
    ]),
  };
}

/** The workspace's crates: every member the root Cargo.toml lists, globs expanded. */
export function workspace(): Crate[] {
  const members = ((parseToml(read('Cargo.toml')).workspace as Table).members as string[]).flatMap(
    (member) => {
      if (!member.includes('*')) return [member];
      const pattern = new RegExp(`^${member.replaceAll('*', '[^/]+')}/Cargo\\.toml$`);
      return files.filter((file) => pattern.test(file)).map((file) => path.posix.dirname(file));
    },
  );
  return members
    .filter((member) => exists(`${member}/Cargo.toml`))
    .map((member) => readCrate(`${member}/Cargo.toml`));
}

/**
 * The folders of the workspace's crates that a crate depends on, named or by path; `dev`
 * picks its dev-dependencies instead. A path outside the workspace's crates counts too.
 */
export function localDependencies(crate: Crate, crates: Crate[], dev = false): string[] {
  return crate.dependencies
    .filter((dependency) => dependency.dev === dev)
    .flatMap((dependency) => {
      const local = crates.find(
        (other) =>
          other.name === dependency.name || (dependency.path && other.dir === dependency.path),
      );
      return local ? [local.dir] : dependency.path ? [dependency.path] : [];
    });
}
