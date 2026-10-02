import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { routeMeta, type RouteMeta } from '../../dashboard/src/app/route-meta.js';
import {
  dependencies,
  moduleSpecifiers,
  resolveModule,
} from '../../tooling/testing/source-analysis.js';

const source = 'dashboard/src';

type Module = { file: string; dependencies: ReturnType<typeof dependencies>; imports: string[] };

function walk(directory: string): string[] {
  return fs
    .readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) =>
      entry.isDirectory()
        ? walk(path.join(directory, entry.name))
        : /\.tsx?$/.test(entry.name)
          ? [path.join(directory, entry.name)]
          : [],
    );
}

// Every module under dashboard/src with the dashboard modules it imports, type-only
// imports included, as paths relative to dashboard/src.
const root = path.resolve(source);
const modules: Module[] = walk(source).map((file) => {
  const references = dependencies(file);
  return {
    file: path.relative(source, file),
    dependencies: references,
    imports: references.flatMap(({ resolved }) =>
      resolved?.startsWith(root + path.sep) ? [path.relative(root, resolved)] : [],
    ),
  };
});
const area = (file: string) => file.split(path.sep)[0]!;
const feature = (file: string) =>
  area(file) === 'features' ? file.split(path.sep)[1]! : undefined;
const edges = modules.flatMap(({ file, imports }) => imports.map((target) => ({ file, target })));

test('sign-in, DSP onboarding and member profiles own their screen dependencies', () => {
  const auth = path.resolve(source, 'features/auth');
  const screens = ['sign-in', 'dsp-onboarding', 'member-profile'];
  for (const screen of screens) {
    const directory = path.join(auth, screen);
    const files = modules.filter(({ file }) =>
      path.join(root, file).startsWith(directory + path.sep),
    );
    assert(files.length > 0, `${screen} must have its own screen directory`);
    for (const { file, dependencies: references } of files) {
      for (const { specifier, target } of references)
        if (target?.startsWith(auth + path.sep))
          assert(
            target.startsWith(directory + path.sep),
            `${file} imports ${specifier}; each auth screen owns its forms, layouts, styles and artwork`,
          );
    }
  }
});

// ui/ holds building blocks that would make sense unchanged in another app.
test('ui components know nothing about the product', () => {
  const files = modules.filter(({ file }) => area(file) === 'ui');
  assert(files.length > 0, 'ui must contain its building blocks');
  for (const { file, dependencies: references } of files)
    for (const { specifier, target } of references)
      if (target)
        assert(
          [path.join(root, 'ui'), path.join(root, 'lib')].some(
            (directory) => target.startsWith(directory + path.sep) || target === directory,
          ),
          `${file} imports ${specifier}; ui may import packages, ui and lib only`,
        );
});

// lib/ is pure logic: no React product code, nothing from app, features, shell or ui.
test('lib depends on nothing else in the dashboard', () => {
  const files = modules.filter(({ file }) => area(file) === 'lib');
  assert(files.length > 0, 'lib must contain its shared logic');
  for (const { file, dependencies: references } of files)
    for (const { specifier, target } of references)
      if (target)
        assert(
          [path.join(root, 'lib'), path.resolve('shared/contracts')].some(
            (directory) => target.startsWith(directory + path.sep) || target === directory,
          ),
          `${file} imports ${specifier}; lib may import packages, lib and shared contracts only`,
        );
});

// A feature that embeds another names it here, so a new edge is a visible decision.
const embeds = ['settings -> connections', 'settings -> driver-match'];
// Named public entries keep unrelated pages out of each other's lazy chunks. This
// remains an explicit boundary: callers cannot reach arbitrary feature internals.
const extraEntries = [
  'features/platform/picker.ts',
  'features/platform/diagnostics/index.ts',
  'features/timecard/settings/index.ts',
  'features/driver-match/badge.ts',
];

test('a feature reaches another feature only through a public entry, and only where allowed', () => {
  assert(
    modules.some(({ file }) => feature(file)),
    'features must contain their screens',
  );
  const found = new Set<string>();
  for (const { file, target } of edges) {
    const to = feature(target);
    if (!to || to === feature(file)) continue;
    assert(
      target === path.join('features', to, 'index.ts') || extraEntries.includes(target),
      `${file} imports ${target}; use the feature's index or a declared public entry`,
    );
    const from = feature(file);
    if (!from) continue;
    assert(
      embeds.includes(`${from} -> ${to}`),
      `${file} imports the ${to} feature; move what they share to ui, lib or app, or allow the edge`,
    );
    found.add(`${from} -> ${to}`);
  }
  assert.deepEqual([...found].sort(), embeds, 'an allowed feature edge is no longer used');
});

test('features build on ui, lib and app only', () => {
  for (const { file, target } of edges)
    if (feature(file))
      assert(
        ['features', 'ui', 'lib', 'app'].includes(area(target)),
        `${file} imports ${target}; a feature may import ui, lib, app and itself only`,
      );
});

test('only the route table and the entry point know the features', () => {
  for (const { file, target } of edges)
    if (feature(target) && !feature(file))
      assert(
        file === path.join('app', 'routes.tsx') || file === 'main.tsx',
        `${file} imports ${target}; outside features only app/routes.tsx and main.tsx may`,
      );
  for (const { file, target } of edges)
    if (area(file) === 'shell')
      assert(
        ['shell', 'ui', 'lib', 'app'].includes(area(target)),
        `${file} imports ${target}; the shell may import ui, lib and app only`,
      );
});

test('no dashboard modules import each other in a cycle', () => {
  const graph = new Map(modules.map(({ file, imports }) => [file, imports]));
  const done = new Set<string>();
  const trail: string[] = [];
  const visit = (file: string) => {
    if (done.has(file)) return;
    const at = trail.indexOf(file);
    assert(at < 0, `import cycle: ${[...trail.slice(at), file].join(' -> ')}`);
    trail.push(file);
    for (const target of graph.get(file) ?? []) visit(target);
    trail.pop();
    done.add(file);
  };
  for (const file of graph.keys()) visit(file);
});

test('every route is declared once and every parent is a route', () => {
  const entries: readonly RouteMeta[] = routeMeta;
  assert(entries.length > 0, 'the route table must declare its screens');
  for (const entry of entries) {
    assert(entry.id && entry.scope, `unreadable route entry: ${JSON.stringify(entry)}`);
    assert.equal(
      entries.filter((other) => other.scope === entry.scope && other.id === entry.id).length,
      1,
      `${entry.scope} route ${entry.id} is declared more than once`,
    );
    if (entry.parent)
      assert(
        entries.some((other) => other.scope === entry.scope && other.id === entry.parent),
        `${entry.id} names ${entry.parent} as its parent, which is not a ${entry.scope} route`,
      );
  }
});

// Shared wire types and non-browser programs cannot depend on screen implementations.
test('contracts, tooling and services are independent of the dashboard', () => {
  for (const directory of ['shared', 'tooling', 'services', 'backend/src']) {
    const files = fs
      .readdirSync(directory, { recursive: true, encoding: 'utf8' })
      .filter((name) => /\.(tsx?|m?js|rs)$/.test(name))
      .map((name) => path.join(directory, name));
    for (const file of files) {
      // Rust embeds use macros, while TS/JS dependencies use the compiler's AST.
      const targets = file.endsWith('.rs')
        ? [
            ...fs.readFileSync(file, 'utf8').matchAll(/include_(?:str|bytes)!\s*\(\s*"([^"]+)"/g),
          ].map((match) => match[1]!)
        : dependencies(file)
            .map(({ specifier }) => specifier)
            .filter((target) => target.startsWith('.'));
      for (const target of targets) {
        const resolved = path.resolve(path.dirname(file), target!);
        assert(
          !resolved.startsWith(path.resolve('dashboard') + path.sep),
          `${file} imports ${target}; move runtime-neutral types into shared/contracts`,
        );
      }
    }
  }
});

// These syntaxes used to escape the regex graph; comments are not dependencies.
test('the architecture graph finds imports, exports, lazy imports and directory indexes', () => {
  assert.deepEqual(
    moduleSpecifiers(
      `
    import type { A } from "./types.js";
    import './style.css';
    export { B } from './reexport.js';
    export * from "./all.js";
    const lazy = () => import("./lazy.js");
    type T = import('./type-only.js').T;
    import legacy = require('./legacy.js');
    // import ignored from './comment.js';
    const unrelated = "from './text.js'";
  `,
      'fixture.ts',
    ),
    [
      './types.js',
      './style.css',
      './reexport.js',
      './all.js',
      './lazy.js',
      './type-only.js',
      './legacy.js',
    ],
  );
  const file = path.join(source, 'main.tsx');
  assert.equal(resolveModule(file, './ui'), path.join(root, 'ui/index.ts'));
  assert.equal(resolveModule(file, './app/routes.js'), path.join(root, 'app/routes.tsx'));
  assert.equal(resolveModule(file, './lib/format.js?raw'), path.join(root, 'lib/format.ts'));
});
