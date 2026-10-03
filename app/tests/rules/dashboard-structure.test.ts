import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { routeMeta, type RouteMeta } from '../../frontend/route-meta.js';
import {
  dependencies,
  moduleSpecifiers,
  resolveModule,
} from '../../../tooling/testing/source-analysis.js';

// Every owner's frontend/: the app's entry, core's parts, the features and the collectors.
const frontends = [
  'app/frontend',
  ...['core', 'features', 'collectors'].flatMap((top) =>
    fs
      .readdirSync(top)
      .map((name) => path.join(top, name, 'frontend'))
      .filter((directory) => fs.existsSync(directory)),
  ),
];
const SHELL = path.join('core', 'shell', 'frontend');

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

const root = path.resolve('.');
const owner = (file: string) =>
  frontends.find((directory) => file.startsWith(directory + path.sep));
// Every frontend module with the frontend modules it imports, type-only imports included,
// as paths relative to the repository.
const modules: Module[] = frontends.flatMap(walk).map((file) => {
  const references = dependencies(file);
  return {
    file,
    dependencies: references,
    imports: references.flatMap(({ resolved }) => {
      const target = resolved && path.relative(root, resolved);
      return target && owner(target) ? [target] : [];
    }),
  };
});
/** app, core, features or collectors. */
const layer = (file: string) => owner(file)!.split(path.sep)[0]!;
/** The shell's own areas: shell, ui, lib, runtime, styles. */
const shellArea = (file: string) =>
  file.startsWith(SHELL + path.sep) ? file.slice(SHELL.length + 1).split(path.sep)[0] : undefined;
/** A screen owner: a feature's, a collector's or a core part's frontend, but not the shell's. */
const unit = (file: string) => {
  const directory = owner(file);
  return directory === SHELL || directory === 'app/frontend' ? undefined : directory;
};
const edges = modules.flatMap(({ file, imports }) => imports.map((target) => ({ file, target })));

// Edges the conversion removes when it splits these files: core naming a feature or the app,
// and DVIC borrowing Timecard's date helpers (localDate and shiftDate go to core).
const pending = [
  'core/platform_owner/frontend/audit/wording.ts -> features/timecard/frontend/paycom.ts',
  'core/shell/frontend/lib/format.ts -> features/timecard/frontend/meal-breaks.ts',
  'core/shell/frontend/lib/format.ts -> features/timecard/frontend/paycom.ts',
  'core/shell/frontend/runtime/navigation.ts -> app/frontend/route-meta.ts',
  'core/shell/frontend/runtime/route-prefetch.ts -> features/timecard/frontend/paycom-date.ts',
  'core/shell/frontend/shell/Shell.tsx -> app/frontend/route-meta.ts',
  'features/dvic/frontend/DvicPage.tsx -> features/timecard/frontend/meal-breaks.ts',
  'features/dvic/frontend/dvic.ts -> features/timecard/frontend/meal-breaks.ts',
];
const isPending = (file: string, target: string) => pending.includes(`${file} -> ${target}`);

test('the pending edges are still there, so the list only shrinks', () => {
  const found = new Set(edges.map(({ file, target }) => `${file} -> ${target}`));
  for (const edge of pending) assert(found.has(edge), `${edge} is gone; remove it from pending`);
});

test('sign-in, DSP onboarding and member profiles own their screen dependencies', () => {
  const accounts = path.resolve('core/accounts/frontend');
  // The one module the screens share: where a signed-in user goes next.
  const shared = /[\\/]sign-in-handoff\.(js|ts)$/;
  const screens = ['sign-in', 'dsp-onboarding', 'member-profile'];
  for (const screen of screens) {
    const directory = path.join(accounts, screen);
    const files = modules.filter(({ file }) => path.resolve(file).startsWith(directory + path.sep));
    assert(files.length > 0, `${screen} must have its own screen directory`);
    for (const { file, dependencies: references } of files) {
      for (const { specifier, target } of references)
        if (target?.startsWith(accounts + path.sep))
          assert(
            target.startsWith(directory + path.sep) ||
              (shared.test(target) && path.dirname(target) === accounts),
            `${file} imports ${specifier}; each accounts screen owns its forms, layouts, styles and artwork`,
          );
    }
  }
});

// The shell's ui/ holds building blocks that would make sense unchanged in another app.
test('ui components know nothing about the product', () => {
  const files = modules.filter(({ file }) => shellArea(file) === 'ui');
  assert(files.length > 0, 'ui must contain its building blocks');
  for (const { file, dependencies: references } of files)
    for (const { specifier, target } of references)
      if (target)
        assert(
          [path.resolve(SHELL, 'ui'), path.resolve(SHELL, 'lib')].some(
            (directory) => target.startsWith(directory + path.sep) || target === directory,
          ),
          `${file} imports ${specifier}; ui may import packages, ui and lib only`,
        );
});

// The shell's lib/ is pure logic: no React product code, nothing from the runtime, shell or ui.
test('lib depends on nothing else in the frontend', () => {
  const files = modules.filter(({ file }) => shellArea(file) === 'lib');
  assert(files.length > 0, 'lib must contain its shared logic');
  for (const { file, dependencies: references } of files)
    for (const { specifier, target, resolved } of references)
      if (target && !(resolved && isPending(file, path.relative(root, resolved))))
        assert(
          [path.resolve(SHELL, 'lib'), path.resolve('shared/contracts')].some(
            (directory) => target.startsWith(directory + path.sep) || target === directory,
          ),
          `${file} imports ${specifier}; lib may import packages, lib and shared contracts only`,
        );
});

// A feature that embeds another names it here, so a new edge is a visible decision.
const embeds = [
  'features/settings -> features/driver_match',
  'features/settings -> features/routes',
];
// Named public entries keep unrelated pages out of each other's lazy chunks. This remains an
// explicit boundary: callers cannot reach arbitrary internals of another owner.
const entries = [
  'core/accounts/frontend/settings/ProfileBadge.tsx',
  'core/accounts/frontend/settings/SecuritySettings.tsx',
  'core/accounts/frontend/settings/ThemeSection.tsx',
  'core/platform_owner/frontend/agents/index.ts',
  'core/platform_owner/frontend/audit/index.ts',
  'core/platform_owner/frontend/diagnostics/index.ts',
  'core/platform_owner/frontend/dsps/index.ts',
  'core/platform_owner/frontend/dsps/picker.ts',
  'features/driver_match/frontend/badge.ts',
  'features/routes/frontend/settings/RouteDataSettings.tsx',
  'features/timecard/frontend/settings/index.ts',
];

test('an owner reaches another only through a public entry, and a feature another only where allowed', () => {
  assert(
    modules.some(({ file }) => layer(file) === 'features'),
    'features must contain their screens',
  );
  const found = new Set<string>();
  for (const { file, target } of edges) {
    const to = unit(target);
    if (!to || to === unit(file) || isPending(file, target)) continue;
    assert(
      target === path.join(to, 'index.ts') || entries.includes(target),
      `${file} imports ${target}; use the owner's index or a declared public entry`,
    );
    if (layer(file) !== 'features' || layer(target) !== 'features') continue;
    const edge = `${path.dirname(unit(file)!)} -> ${path.dirname(to)}`;
    assert(
      embeds.includes(edge),
      `${file} imports ${target}; move what they share to core, or allow the edge`,
    );
    found.add(edge);
  }
  assert.deepEqual([...found].sort(), embeds, 'an allowed feature edge is no longer used');
});

test("features and collectors build on core's ui, lib, runtime and public entries only", () => {
  for (const { file, target } of edges) {
    if (isPending(file, target) || owner(target) === owner(file)) continue;
    if (layer(file) === 'features')
      assert(
        ['core', 'features'].includes(layer(target)) && shellArea(target) !== 'shell',
        `${file} imports ${target}; a feature may import core's ui, lib, runtime and public entries, and itself`,
      );
    if (layer(file) === 'collectors')
      assert(
        layer(target) === 'core' && shellArea(target) !== 'shell',
        `${file} imports ${target}; a collector's frontend may import core and itself only`,
      );
  }
});

test('only the route table and the entry point know the features, and core knows only core', () => {
  for (const { file, target } of edges) {
    if (isPending(file, target)) continue;
    if (['features', 'collectors'].includes(layer(target)) && layer(target) !== layer(file))
      assert(
        file === path.join('app', 'frontend', 'routes.tsx') ||
          file === path.join('app', 'frontend', 'main.tsx'),
        `${file} imports ${target}; outside features only app/frontend/routes.tsx and main.tsx may`,
      );
    if (layer(file) === 'core')
      assert(layer(target) === 'core', `${file} imports ${target}; core may import core only`);
    if (shellArea(file) === 'shell')
      assert(
        owner(target) === SHELL,
        `${file} imports ${target}; the shell may import the shell's ui, lib and runtime only`,
      );
  }
});

test('no frontend modules import each other in a cycle', () => {
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

// Shared wire types, Rust and non-browser programs cannot depend on screen implementations.
// Tests may: they live in each owner's tests/ and exercise its frontend logic.
test('contracts, tooling, services and backends are independent of the frontends', () => {
  const frontendRoots = frontends.map((directory) => path.resolve(directory) + path.sep);
  for (const directory of [
    'shared',
    'tooling',
    'services',
    'app',
    'core',
    'collectors',
    'features',
  ]) {
    const files = fs
      .readdirSync(directory, { recursive: true, encoding: 'utf8' })
      .filter((name) => /\.(tsx?|m?js|rs)$/.test(name))
      .map((name) => path.join(directory, name))
      .filter(
        (file) => !owner(file) && (file.endsWith('.rs') || !/(^|[\\/])tests[\\/]/.test(file)),
      );
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
          !frontendRoots.some((directory) => resolved.startsWith(directory)),
          `${file} imports ${target}; move runtime-neutral types into shared/contracts`,
        );
        // The Rust build cache leaves TypeScript and CSS out of its fingerprint.
        if (file.endsWith('.rs'))
          assert(
            !/\.(tsx?|mts|cts|css)$/.test(resolved),
            `${file} embeds ${target}; Rust may not embed TypeScript or CSS`,
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
  const file = path.join(SHELL, 'index.ts');
  assert.equal(resolveModule(file, './ui'), path.resolve(SHELL, 'ui/index.ts'));
  assert.equal(resolveModule(file, './shell/Shell.js'), path.resolve(SHELL, 'shell/Shell.tsx'));
  assert.equal(resolveModule(file, './lib/format.js?raw'), path.resolve(SHELL, 'lib/format.ts'));
});
