import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { routeMeta, type RouteMeta } from '../../frontend/route-meta.js';
import {
  dependencies,
  eagerSpecifiers,
  lazySpecifiers,
  moduleSpecifiers,
  resolveModule,
} from '../../../tooling/testing/source-analysis.js';

// Every owner: the app, core's parts, the features and the collectors. An owner's frontend is
// its frontend/ and its api/client.ts, the frontend's end of its API.
const owners = [
  'app',
  ...['core', 'features', 'collectors'].flatMap((top) =>
    fs.readdirSync(top).map((name) => path.join(top, name)),
  ),
];
const frontends = owners
  .map((directory) => path.join(directory, 'frontend'))
  .filter((directory) => fs.existsSync(directory));
const clients = owners
  .map((directory) => path.join(directory, 'api', 'client.ts'))
  .filter((file) => fs.existsSync(file));
const SHELL = path.join('core', 'shell', 'frontend');
const APP = path.join('app', 'frontend');
const FEATURES = path.join(APP, 'features.ts');

type Module = {
  file: string;
  dependencies: ReturnType<typeof dependencies>;
  imports: string[];
  eager: string[];
  lazy: string[];
};

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
/** The owner whose frontend a file belongs to, as its directory. */
function owner(file: string) {
  if (clients.includes(file)) return path.dirname(path.dirname(file));
  const frontend = frontends.find((directory) => file.startsWith(directory + path.sep));
  return frontend && path.dirname(frontend);
}
/** The frontend modules among `specifiers`, as paths relative to the repository. */
const frontendModules = (file: string, specifiers: string[]) =>
  specifiers.flatMap((specifier) => {
    const resolved = resolveModule(file, specifier);
    const target = resolved && path.relative(root, resolved);
    return target && owner(target) ? [target] : [];
  });
// Every frontend module with the frontend modules it imports (type-only and lazy imports
// included), those it loads with it, and those it loads lazily.
const modules: Module[] = [...frontends.flatMap(walk), ...clients].map((file) => {
  const text = fs.readFileSync(file, 'utf8');
  return {
    file,
    dependencies: dependencies(file),
    imports: frontendModules(file, moduleSpecifiers(text, file)),
    eager: frontendModules(file, eagerSpecifiers(text, file)),
    lazy: frontendModules(file, lazySpecifiers(text, file)),
  };
});
/** app, core, features or collectors. */
const layer = (file: string) => owner(file)!.split(path.sep)[0]!;
/** The shell's own areas: shell, ui, lib, runtime, styles. */
const shellArea = (file: string) =>
  file.startsWith(SHELL + path.sep) ? file.slice(SHELL.length + 1).split(path.sep)[0] : undefined;
/** A screen owner: a feature, a collector or a core part, but not the shell or the app. */
const unit = (file: string) => {
  const directory = owner(file);
  return directory === path.dirname(SHELL) || directory === 'app' ? undefined : directory;
};
const edges = modules.flatMap(({ file, imports }) => imports.map((target) => ({ file, target })));
/** An owner's front doors: its manifest, its lazily loaded page and its API client. */
const doors = (directory: string) => [
  path.join(directory, 'frontend', 'feature.ts'),
  path.join(directory, 'frontend', 'index.ts'),
  path.join(directory, 'api', 'client.ts'),
];

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
    for (const { specifier, target } of references)
      if (target)
        assert(
          [path.resolve(SHELL, 'lib'), path.resolve('shared/contracts')].some(
            (directory) => target.startsWith(directory + path.sep) || target === directory,
          ),
          `${file} imports ${specifier}; lib may import packages, lib and shared contracts only`,
        );
});

// Besides each owner's front doors, named public entries keep unrelated pages out of each
// other's lazy chunks. This remains an explicit boundary: callers cannot reach arbitrary
// internals of another owner.
const entries = [
  // The account's Settings panels, which the platform owner's Settings page shows too.
  'core/accounts/frontend/settings/tabs.ts',
];

test('an owner reaches another only through a front door or public entry, and a feature never another', () => {
  assert(
    modules.some(({ file }) => layer(file) === 'features'),
    'features must contain their screens',
  );
  for (const { file, target } of edges) {
    const to = unit(target);
    if (!to || to === unit(file)) continue;
    assert(
      doors(to).includes(target) || entries.includes(target),
      `${file} imports ${target}; use the owner's feature.ts, index.ts, api/client.ts or a declared public entry`,
    );
    // Features meet only in the slots their hosts offer.
    assert(
      layer(file) !== 'features' || layer(target) !== 'features',
      `${file} imports ${target}; move what they share to core, or fill a slot`,
    );
  }
});

test("features and collectors build on core's ui, lib, runtime and public entries only", () => {
  for (const { file, target } of edges) {
    if (owner(target) === owner(file)) continue;
    if (layer(file) === 'features')
      assert(
        layer(target) === 'core' && shellArea(target) !== 'shell',
        `${file} imports ${target}; a feature may import core's ui, lib, runtime and public entries, and itself`,
      );
    if (layer(file) === 'collectors')
      assert(
        layer(target) === 'core' && shellArea(target) !== 'shell',
        `${file} imports ${target}; a collector's frontend may import core and itself only`,
      );
    // The platform owner's dashboard hosts slots that features and collectors fill from their
    // manifests; neither reaches into it.
    if (['features', 'collectors'].includes(layer(file)))
      assert(
        owner(target) !== path.join('core', 'platform_owner'),
        `${file} imports ${target}; fill the platform owner's slots from the manifest instead`,
      );
  }
});

test('only the app lists the features, and core knows only core', () => {
  for (const { file, target } of edges) {
    if (['features', 'collectors'].includes(layer(target)) && layer(target) !== layer(file))
      assert(
        file === FEATURES,
        `${file} imports ${target}; outside features only ${FEATURES} may, through their manifests`,
      );
    if (layer(file) === 'core')
      assert(layer(target) === 'core', `${file} imports ${target}; core may import core only`);
    if (shellArea(file) === 'shell')
      assert(
        shellArea(target),
        `${file} imports ${target}; the shell may import the shell's ui, lib and runtime only`,
      );
  }
});

// A manifest is loaded up front, so it stays small: the pages it names load when opened.
test('every owner with screens has a manifest, the app lists them all, and each loads its pages lazily', () => {
  const manifests = modules.filter(({ file }) => path.basename(file) === 'feature.ts');
  for (const directory of frontends.filter((frontend) => ![SHELL, APP].includes(frontend)))
    assert(
      fs.existsSync(path.join(directory, 'feature.ts')),
      `${directory} has screens; declare them in ${directory}/feature.ts`,
    );
  for (const { file } of manifests)
    assert.equal(file, path.join(owner(file)!, 'frontend', 'feature.ts'));
  const listed = modules.find(({ file }) => file === FEATURES)!.eager;
  assert.deepEqual(
    [...listed].sort(),
    manifests.map(({ file }) => file).sort(),
    `${FEATURES} lists exactly every owner's frontend/feature.ts`,
  );
  const graph = new Map(modules.map(({ file, eager }) => [file, eager]));
  for (const { file, eager, lazy } of manifests) {
    const loaded = new Set<string>();
    const load = (module: string) => {
      if (loaded.has(module)) return;
      loaded.add(module);
      for (const next of graph.get(module) ?? []) load(next);
    };
    for (const module of eager) load(module);
    const index = path.join(owner(file)!, 'frontend', 'index.ts');
    for (const page of [index, ...lazy])
      assert(!loaded.has(page), `${file} loads ${page} with it; import the page lazily`);
    for (const module of loaded)
      assert(
        !(module.endsWith('.tsx') && owner(module) === owner(file)),
        `${file} loads the component ${module} with it; import it lazily`,
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
  const clientFiles = clients.map((file) => path.resolve(file));
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
          !frontendRoots.some((directory) => resolved.startsWith(directory)) &&
            !clientFiles.includes(resolved.replace(/\.js$/, '.ts')),
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
  const text = `
    import type { A } from "./types.js";
    import './style.css';
    export { B } from './reexport.js';
    export * from "./all.js";
    const lazy = () => import("./lazy.js");
    type T = import('./type-only.js').T;
    import legacy = require('./legacy.js');
    import { type C, D } from './mixed.js';
    export type { E } from './type-export.js';
    // import ignored from './comment.js';
    const unrelated = "from './text.js'";
  `;
  assert.deepEqual(moduleSpecifiers(text, 'fixture.ts'), [
    './types.js',
    './style.css',
    './reexport.js',
    './all.js',
    './lazy.js',
    './type-only.js',
    './legacy.js',
    './mixed.js',
    './type-export.js',
  ]);
  // What a module loads with it: neither its lazy imports nor its type-only ones.
  assert.deepEqual(eagerSpecifiers(text, 'fixture.ts'), [
    './style.css',
    './reexport.js',
    './all.js',
    './mixed.js',
  ]);
  assert.deepEqual(lazySpecifiers(text, 'fixture.ts'), ['./lazy.js']);
  const file = path.join(SHELL, 'index.ts');
  assert.equal(resolveModule(file, './ui'), path.resolve(SHELL, 'ui/index.ts'));
  assert.equal(resolveModule(file, './shell/Shell.js'), path.resolve(SHELL, 'shell/Shell.tsx'));
  assert.equal(resolveModule(file, './lib/format.js?raw'), path.resolve(SHELL, 'lib/format.ts'));
});
