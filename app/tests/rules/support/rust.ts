import path from 'node:path';
import { workspace, type Crate } from './cargo.js';
import { isFile, join, read } from './repo.js';

/** A string literal: where it sits, quotes included, and the text it stands for. */
export type Literal = { start: number; end: number; value: string };
/**
 * A Rust file with its comments and the insides of its string and character literals blanked
 * out, offsets and lines kept, so that code can be searched without matching prose.
 */
export type Lexed = { text: string; masked: string; literals: Literal[] };

const escapes: Record<string, string> = { n: '\n', r: '\r', t: '\t', '0': '\0' };
function unescape(body: string): string {
  return body.replace(/\\(u\{([0-9a-fA-F]+)\}|x([0-9a-fA-F]{2})|\n\s*|.)/g, (_, escape, u, x) =>
    u
      ? String.fromCodePoint(parseInt(u, 16))
      : x
        ? String.fromCharCode(parseInt(x, 16))
        : escape.startsWith('\n')
          ? ''
          : (escapes[escape] ?? escape),
  );
}

const lexed = new Map<string, Lexed>();
export function lex(text: string): Lexed {
  const out = text.split('');
  const literals: Literal[] = [];
  const blank = (from: number, to: number) => {
    for (let i = from; i < to; i++) if (out[i] !== '\n') out[i] = ' ';
  };
  const word = (i: number) => i > 0 && /[A-Za-z0-9_]/.test(text[i - 1]!);
  let i = 0;
  while (i < text.length) {
    if (text.startsWith('//', i)) {
      const end = text.indexOf('\n', i);
      const stop = end < 0 ? text.length : end;
      blank(i, stop);
      i = stop;
    } else if (text.startsWith('/*', i)) {
      let depth = 0;
      let j = i;
      do {
        if (text.startsWith('/*', j)) (depth++, (j += 2));
        else if (text.startsWith('*/', j)) (depth--, (j += 2));
        else j++;
      } while (depth > 0 && j < text.length);
      blank(i, j);
      i = j;
    } else if ('br'.includes(text[i]!) && /^b?r#*"/.test(text.slice(i, i + 40)) && !word(i)) {
      const open = /^b?r(#*)"/.exec(text.slice(i, i + 40))!;
      const close = `"${open[1]}`;
      const start = i + open[0].length;
      const end = text.indexOf(close, start);
      const stop = end < 0 ? text.length : end;
      literals.push({ start: i, end: stop + close.length, value: text.slice(start, stop) });
      blank(start, stop);
      i = stop + close.length;
    } else if (text[i] === '"' || (text[i] === 'b' && text[i + 1] === '"' && !word(i))) {
      const start = text[i] === '"' ? i + 1 : i + 2;
      let j = start;
      while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1;
      literals.push({ start: start - 1, end: j + 1, value: unescape(text.slice(start, j)) });
      blank(start, j);
      i = j + 1;
    } else if (text[i] === "'") {
      // A character literal, or a lifetime or label, which has no closing quote.
      const escaped = /^'\\(u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)'/.exec(text.slice(i, i + 12));
      const plain = /^'([^\\'\n])'/u.exec(text.slice(i, i + 4));
      const length = (escaped ?? plain)?.[0].length;
      if (length) {
        blank(i + 1, i + length - 1);
        i += length;
      } else i++;
    } else i++;
  }
  return { text, masked: out.join(''), literals };
}
export function lexFile(file: string): Lexed {
  let result = lexed.get(file);
  if (!result) lexed.set(file, (result = lex(read(file))));
  return result;
}
/** The literal that starts at `offset`, after any whitespace. */
export function literalAt(source: Lexed, offset: number): Literal | undefined {
  while (/\s/.test(source.masked[offset] ?? '')) offset++;
  return source.literals.find((literal) => literal.start === offset);
}
/** The offset of the bracket that closes the one at `open`. */
export function closing(masked: string, open: number): number {
  const pairs: Record<string, string> = { '{': '}', '(': ')', '[': ']' };
  const stack: string[] = [];
  for (let i = open; i < masked.length; i++) {
    const char = masked[i]!;
    if (pairs[char]) stack.push(pairs[char]!);
    else if (char === stack.at(-1)) {
      stack.pop();
      if (!stack.length) return i;
    }
  }
  return masked.length;
}
export const lineOf = (text: string, offset: number) => text.slice(0, offset).split('\n').length;

/** A path a `use` declaration brings into scope, and the name it is known by there. */
export type UsePath = { segments: string[]; alias?: string; glob: boolean; public?: boolean };
/** Flattens a use tree such as `crate::{a, b::{c as d, *}}`. */
export function useTree(tree: string): UsePath[] {
  const out: UsePath[] = [];
  const walk = (prefix: string[], text: string) => {
    let depth = 0;
    let part = '';
    const parts: string[] = [];
    for (const char of text) {
      if (char === '{') depth++;
      if (char === '}') depth--;
      if (char === ',' && depth === 0) (parts.push(part), (part = ''));
      else part += char;
    }
    parts.push(part);
    for (const raw of parts.map((item) => item.trim()).filter(Boolean)) {
      const brace = raw.indexOf('{');
      if (brace >= 0) {
        const head = raw.slice(0, brace).replace(/::\s*$/, '');
        const inner = raw.slice(brace + 1, raw.lastIndexOf('}'));
        walk(
          [
            ...prefix,
            ...head
              .split('::')
              .map((s) => s.trim())
              .filter(Boolean),
          ],
          inner,
        );
        continue;
      }
      const [item, alias] = raw.split(/\s+as\s+/);
      const segments = [
        ...prefix,
        ...item!
          .split('::')
          .map((s) => s.trim())
          .filter(Boolean),
      ];
      const named = alias ? { alias } : {};
      if (segments.at(-1) === '*') out.push({ segments: segments.slice(0, -1), glob: true });
      else if (segments.at(-1) === 'self')
        out.push({ segments: segments.slice(0, -1), ...named, glob: false });
      else out.push({ segments, ...named, glob: false });
    }
  };
  walk([], tree.replace(/\s+/g, ' '));
  return out;
}

export type ModDeclaration = {
  name: string;
  start: number;
  end: number;
  /** The `#[path]` it gives, if any. */
  path?: string;
  test: boolean;
  /** `mod name { … }`: the offsets of its braces. */
  inline?: { open: number; close: number };
};
/** Every `mod` declaration in a file, with its `#[path]` and whether it is `#[cfg(test)]`. */
export function modDeclarations(source: Lexed): ModDeclaration[] {
  const pattern =
    /((?:#!?\[[^\]]*\]\s*)*)(?<![A-Za-z0-9_])(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*([;{])/g;
  return [...source.masked.matchAll(pattern)].map((match) => {
    const attributes = match[1]!;
    const pathAttribute = /#\[\s*path\s*=\s*/.exec(attributes);
    const literal =
      pathAttribute &&
      literalAt(source, match.index + pathAttribute.index + pathAttribute[0].length);
    const end = match.index + match[0].length;
    return {
      name: match[2]!,
      start: match.index + attributes.length,
      end,
      ...(literal ? { path: literal.value } : {}),
      test: /cfg\s*\(\s*test\s*\)/.test(attributes),
      ...(match[3] === '{'
        ? { inline: { open: end - 1, close: closing(source.masked, end - 1) } }
        : {}),
    };
  });
}

/** `include_str!`/`include_bytes!` targets, relative to the file that names them. */
export function includes(file: string): { target: string; offset: number }[] {
  const source = lexFile(file);
  return [...source.masked.matchAll(/\binclude_(?:str|bytes)!\s*\(/g)].flatMap((match) => {
    const literal = literalAt(source, match.index + match[0].length);
    return literal ? [{ target: join(file, literal.value), offset: match.index }] : [];
  });
}
/** Where `#[test]` and `#[tokio::test]` functions sit. */
export const testAttributes = (source: Lexed) =>
  [...source.masked.matchAll(/#\[\s*(?:tokio\s*::\s*)?test\b/g)].map((match) => match.index);

/** Items a module defines, inside macro calls such as `text_enum! { … }` too. */
const itemPattern =
  /\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:async|unsafe|extern|const)\s+)*(struct|enum|union|fn|const|static|type|trait|mod)\s+([A-Za-z_][A-Za-z0-9_]*)|\bmacro_rules!\s*([A-Za-z_][A-Za-z0-9_]*)/g;

// Brace depth at each offset, and the innermost open brace around it.
type Shape = { depth: Int32Array; enclosing: Int32Array };
const shapes = new Map<string, Shape>();
function shape(file: string): Shape {
  let found = shapes.get(file);
  if (found) return found;
  const { masked } = lexFile(file);
  const depth = new Int32Array(masked.length + 1);
  const enclosing = new Int32Array(masked.length + 1);
  const stack: number[] = [];
  for (let i = 0; i < masked.length; i++) {
    depth[i] = stack.length;
    enclosing[i] = stack.at(-1) ?? -1;
    if (masked[i] === '{') stack.push(i);
    else if (masked[i] === '}') stack.pop();
  }
  depth[masked.length] = stack.length;
  enclosing[masked.length] = -1;
  shapes.set(file, (found = { depth, enclosing }));
  return found;
}

/** One module of one crate target: a file, or an inline `mod` inside one. */
export type Module = {
  crate: string;
  path: string[];
  file: string;
  /** Compiled only for tests. */
  test: boolean;
  parent?: Module;
  /** For an inline module, the offsets of its braces in `file`. */
  span?: { open: number; close: number };
  children: Map<string, Module>;
  items: Set<string>;
  imports: Map<string, UsePath>;
  globs: UsePath[];
  /** A crate root's `#[macro_export]` macros, by the module that defines each. */
  macros?: Map<string, Module>;
  // Where `mod name;` inside it looks for files, as rustc does.
  directory: string;
  relative?: string;
};
/** A module a file names, at `offset`; `mount` when a `mod` declaration mounts its file. */
export type Reference = {
  file: string;
  offset: number;
  to: string;
  module: Module;
  mount?: boolean;
};
export type Root = { crate: Crate; kind: string; name: string; file: string; module: Module };
export type Mount = { file: string; offset: number; test: boolean };
const usePattern = /(?<![A-Za-z0-9_])(?:pub(?:\s*\([^)]*\))?\s+)?use\s+([^;]+);/g;

/**
 * The workspace's Rust as the compiler sees its modules: each crate target's tree, followed
 * through `mod` declarations and their `#[path]`s, and a resolver for the paths code names.
 */
export class Rust {
  readonly crates: Crate[];
  readonly roots: Root[] = [];
  /** Every module a file is mounted as, in any crate target. */
  readonly mounts = new Map<string, Module[]>();
  /** Every `mod` declaration that mounts a file. */
  readonly mounted = new Map<string, Mount[]>();
  private readonly named = new Map<string, Reference[]>();

  constructor(crates = workspace()) {
    this.crates = crates;
    for (const crate of crates)
      for (const target of crate.targets) {
        if (!isFile(target.file)) continue;
        const module = this.module(
          target.name,
          [],
          target.file,
          false,
          path.posix.dirname(target.file),
        );
        this.roots.push({ crate, ...target, module });
        this.fill(module, 0, read(target.file).length);
      }
  }

  private module(
    crate: string,
    at: string[],
    file: string,
    test: boolean,
    directory: string,
    extra: Partial<Module> = {},
  ): Module {
    const module: Module = {
      crate,
      path: at,
      file,
      test,
      children: new Map(),
      items: new Set(),
      imports: new Map(),
      globs: [],
      directory,
      ...extra,
    };
    if (!module.span) this.mounts.set(file, [...(this.mounts.get(file) ?? []), module]);
    return module;
  }

  // Reads one module's body: a whole file, or an inline module's braces.
  private fill(module: Module, from: number, to: number) {
    const source = lexFile(module.file);
    const { depth, enclosing } = shape(module.file);
    const base = depth[from]!;
    const declarations = modDeclarations(source).filter((d) => d.start >= from && d.end <= to);
    const direct = declarations.filter(
      (d) =>
        !declarations.some(
          ({ inline }) => inline && d.start > inline.open && d.end <= inline.close,
        ),
    );
    const inside = (offset: number) =>
      offset >= from &&
      offset < to &&
      !direct.some(({ inline }) => inline && offset > inline.open && offset < inline.close);
    for (const match of source.masked.matchAll(itemPattern)) {
      if (!inside(match.index)) continue;
      const level = depth[match.index]! - base;
      // Items at the top of the body, or inside a macro call there, such as `text_enum!`.
      const open = enclosing[match.index]!;
      const inMacro =
        level === 1 &&
        /[A-Za-z_][A-Za-z0-9_]*!\s*$/.test(source.masked.slice(Math.max(0, open - 60), open));
      if (level === 0 || inMacro) module.items.add((match[2] ?? match[3])!);
      // An exported macro is named from the crate root.
      if (match[3] && /#\[\s*macro_export\s*\]\s*$/.test(source.masked.slice(0, match.index))) {
        let top = module;
        while (top.parent) top = top.parent;
        (top.macros ??= new Map()).set(match[3], module);
      }
    }
    for (const match of source.masked.matchAll(usePattern)) {
      if (!inside(match.index) || depth[match.index]! !== base) continue;
      const exported = /^pub\b/.test(match[0]);
      for (const used of useTree(match[1]!).map((u) => ({ ...u, public: exported }))) {
        if (used.glob) module.globs.push(used);
        else if (used.alias !== '_') module.imports.set(used.alias ?? used.segments.at(-1)!, used);
      }
    }
    for (const declaration of direct) {
      const test = module.test || declaration.test;
      const at = [...module.path, declaration.name];
      const folder = module.relative ? `${module.directory}/${module.relative}` : module.directory;
      if (declaration.inline) {
        const child = this.module(
          module.crate,
          at,
          module.file,
          test,
          `${folder}/${declaration.name}`,
          {
            parent: module,
            span: declaration.inline,
          },
        );
        module.children.set(declaration.name, child);
        this.fill(child, declaration.inline.open + 1, declaration.inline.close);
        continue;
      }
      // A `#[path]` is relative to the declaring file's folder, and its file keeps its own
      // children beside it; `name.rs` keeps them in `name/`, as `name/mod.rs` does.
      let file: string | undefined;
      let relative: string | undefined;
      if (declaration.path) {
        const directory = module.span ? folder : module.directory;
        file = path.posix.normalize(`${directory}/${declaration.path}`);
      } else if (isFile(`${folder}/${declaration.name}.rs`)) {
        file = `${folder}/${declaration.name}.rs`;
        relative = declaration.name;
      } else file = `${folder}/${declaration.name}/mod.rs`;
      if (!isFile(file)) continue;
      // A module tree never holds a file inside itself.
      for (let above: Module | undefined = module; above; above = above.parent)
        if (above.file === file && !above.span) file = undefined;
      if (!file) continue;
      const mounts = this.mounted.get(file) ?? [];
      if (!mounts.some((m) => m.file === module.file && m.offset === declaration.start))
        this.mounted.set(file, [...mounts, { file: module.file, offset: declaration.start, test }]);
      const child = this.module(module.crate, at, file, test, path.posix.dirname(file), {
        parent: module,
        ...(relative ? { relative } : {}),
      });
      module.children.set(declaration.name, child);
      this.fill(child, 0, read(file).length);
    }
  }

  /** The innermost module that `offset` in `file` belongs to, once for each mounting. */
  modulesAt(file: string, offset: number): Module[] {
    return (this.mounts.get(file) ?? []).map((module) => {
      let current = module;
      for (;;) {
        const inner = [...current.children.values()].find(
          (child) =>
            child.span &&
            child.file === file &&
            offset > child.span.open &&
            offset < child.span.close,
        );
        if (!inner) return current;
        current = inner;
      }
    });
  }

  rootOf(module: Module): Root | undefined {
    let top = module;
    while (top.parent) top = top.parent;
    return this.roots.find((root) => root.module === top);
  }
  /** The workspace crates a module's code can name: its dependencies and its own library. */
  externs(module: Module): Map<string, Module> {
    const root = this.rootOf(module);
    const found = new Map<string, Module>();
    if (!root) return found;
    const lib = (crate: Crate) => this.roots.find((r) => r.crate === crate && r.kind === 'lib');
    for (const dependency of root.crate.dependencies) {
      const crate = this.crates.find(
        (other) =>
          other.name === dependency.name || (dependency.path && other.dir === dependency.path),
      );
      const target = crate && lib(crate);
      if (target) found.set(crate!.ident, target.module);
    }
    const own = lib(root.crate);
    if (own && own !== root) found.set(root.crate.ident, own.module);
    return found;
  }

  /**
   * The module a path names in `module`, or the one that defines the item it names, through
   * `use` re-exports and globs. Undefined for paths outside the workspace (std, serde, …)
   * and for names it cannot find.
   */
  resolve(module: Module, segments: string[], depth = 0): Module | undefined {
    if (depth > 16 || !segments.length) return undefined;
    const [first, ...rest] = segments;
    let start: Module | undefined;
    if (first === 'crate' || first === '$crate') {
      start = module;
      while (start.parent) start = start.parent;
    } else if (first === 'self') start = module;
    else if (first === 'super') start = module.parent;
    else {
      const external = this.externs(module).get(first!);
      if (!external) return this.walk(module, segments, depth, true);
      start = external;
    }
    while (start && rest[0] === 'super') {
      start = start.parent;
      rest.shift();
    }
    if (!start) return undefined;
    return rest.length ? this.walk(start, rest, depth, false) : start;
  }
  // Follows names down from `module`, each visible in the module before it.
  private walk(
    module: Module,
    segments: string[],
    depth: number,
    lexical: boolean,
  ): Module | undefined {
    let current = module;
    for (let index = 0; index < segments.length; index++) {
      const name = segments[index]!;
      const child = current.children.get(name);
      if (child) {
        current = child;
        continue;
      }
      const exported = current.macros?.get(name);
      if (exported) return exported;
      if (current.items.has(name)) return current;
      const imported = current.imports.get(name);
      if (imported) {
        const target = this.resolve(current, [...imported.segments], depth + 1);
        const remaining = segments.slice(index + 1);
        if (!target || !remaining.length) return target;
        return this.walk(target, remaining, depth + 1, false) ?? target;
      }
      for (const glob of current.globs) {
        const target = this.resolve(current, [...glob.segments], depth + 1);
        if (target && target !== current && this.names(target, name))
          return this.walk(target, segments.slice(index), depth + 1, false);
      }
      // An unknown first name is outside the workspace: std, serde or a prelude name.
      return lexical && index === 0 ? undefined : current;
    }
    return current;
  }
  // What a glob brings: a module's children, items and public re-exports, not its own
  // private imports.
  private names(module: Module, name: string) {
    return (
      module.children.has(name) || module.items.has(name) || !!module.imports.get(name)?.public
    );
  }

  /**
   * Every module a file's code names: through its `use` trees, the paths it spells from
   * `crate`, `super`, `self` or a workspace crate, and the files its `mod`s mount.
   */
  references(file: string): Reference[] {
    const known = this.named.get(file);
    if (known) return known;
    const source = lexFile(file);
    const out: Reference[] = [];
    this.named.set(file, out);
    const add = (offset: number, target: Module | undefined) => {
      if (target && target.file !== file)
        out.push({ file, offset, to: target.file, module: target });
    };
    const uses: [number, number][] = [];
    for (const match of source.masked.matchAll(usePattern)) {
      uses.push([match.index, match.index + match[0].length]);
      for (const module of this.modulesAt(file, match.index))
        for (const used of useTree(match[1]!)) {
          const target = this.resolve(module, used.segments);
          // A glob of a module that gathers others' items names none of them in particular.
          if (!used.glob || !target?.globs.length) add(match.index, target);
        }
    }
    const externs = new Set(
      (this.mounts.get(file) ?? []).flatMap((m) => [...this.externs(m).keys()]),
    );
    const starts = ['crate', 'super', 'self', ...externs].join('|');
    const pattern = new RegExp(
      `(?<![A-Za-z0-9_:$])((?:${starts})(?:\\s*::\\s*[A-Za-z_][A-Za-z0-9_]*)+)`,
      'g',
    );
    for (const match of source.masked.matchAll(pattern)) {
      if (uses.some(([from, to]) => match.index >= from && match.index < to)) continue;
      const segments = match[1]!.split('::').map((segment) => segment.trim());
      for (const module of this.modulesAt(file, match.index))
        add(match.index, this.resolve(module, segments));
    }
    for (const mount of [...this.mounted.entries()].flatMap(([target, list]) =>
      list.filter((m) => m.file === file).map((m) => ({ target, offset: m.offset })),
    ))
      out.push({
        file,
        offset: mount.offset,
        to: mount.target,
        module: this.mounts.get(mount.target)![0]!,
        mount: true,
      });
    return out;
  }
}

let shared: Rust | undefined;
/** The workspace's module trees, built once per test process. */
export const rust = () => (shared ??= new Rust());
