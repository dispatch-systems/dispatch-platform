import assert from 'node:assert/strict';
import test from 'node:test';
import { holds } from './support/holds.js';
import { declared, inTest } from './support/manifests.js';
import { isTestFile, ownerOf } from './support/repo.js';
import { lexFile, rust } from './support/rust.js';
import { imports, scriptFiles } from './support/typescript.js';

// Each owner's TypeScript reaches only itself, core, and for a feature the features and
// collectors its manifest declares; core reaches only core. Only the app imports features
// and collectors to list them. plans/restructure/enforcement.md, section 3. Front doors
// between frontends are dashboard-structure.test.ts's.
/** The owner, or the top two folders, a file belongs to. */
const unitOf = (file: string) => ownerOf(file)?.dir ?? file.split('/').slice(0, 2).join('/');

test("each owner's TypeScript imports only itself, core and what it declares", () => {
  const reaches = new Set<string>();
  for (const file of scriptFiles()) {
    const owner = ownerOf(file);
    if (!owner || owner.layer === 'app') continue;
    const may = declared(owner);
    const test = isTestFile(file);
    for (const { target } of imports(file)) {
      const to = ownerOf(target);
      if (to?.dir === owner.dir || to?.layer === 'core' || (to && may.has(to.dir))) continue;
      // Tests may run on the tooling's harness and drive the services they exercise.
      if (test && /^(tooling|services)\//.test(target)) continue;
      reaches.add(`${owner.dir}${test ? "'s tests import" : ' imports'} ${unitOf(target)}`);
    }
  }
  holds('encapsulation', 'typescript', reaches);
});

/** The owners whose manifests a file names: in its product code, and in all of its code. */
type Listing = { product: Set<string>; all: Set<string> };
/**
 * Whether a file lists features or collectors as only the app may. A feature's or collector's
 * tests may name the manifests of itself and of what it may use, the dependencies rule's set,
 * to install a registry of just those; core's tests name none.
 */
function listsOthers(file: string, { product, all }: Listing): boolean {
  if (all.size < 2 || file.startsWith('app/')) return false;
  const owner = ownerOf(file);
  if (product.size > 1 || !owner || !['feature', 'collector'].includes(owner.layer)) return true;
  const may = new Set([owner.dir, ...declared(owner)]);
  return [...all].some((dir) => !may.has(dir));
}

// The frontend's list is app/frontend/features.ts; the backend's is the app's registry.
test('only the app lists the features and collectors', () => {
  const listing = new Map<string, Listing>();
  const list = (file: string, manifest: string, test: boolean) => {
    const owner = ownerOf(manifest);
    if (!owner || !['feature', 'collector'].includes(owner.layer)) return;
    const found = listing.get(file) ?? { product: new Set(), all: new Set() };
    found.all.add(owner.dir);
    if (!test) found.product.add(owner.dir);
    listing.set(file, found);
  };
  for (const file of scriptFiles())
    for (const { target } of imports(file))
      if (/^(features|collectors)\/[^/]+\/frontend\/feature\.ts$/.test(target))
        list(file, target, isTestFile(file));
  const manifest = /^(features\/[^/]+\/feature|collectors\/[^/]+\/collector)\.rs$/;
  for (const file of rust().mounts.keys()) {
    // A manifest's file mounted as a module, or its `FEATURE` or `COLLECTOR` named.
    for (const reference of rust().references(file))
      if (reference.mount && manifest.test(reference.to))
        list(file, reference.to, inTest(file, reference.offset));
    const { masked } = lexFile(file);
    for (const match of masked.matchAll(
      /(?<![A-Za-z0-9_:])((?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)+)(?:FEATURE|COLLECTOR)\b/g,
    )) {
      const path = match[1]!
        .split('::')
        .map((segment) => segment.trim())
        .filter(Boolean);
      for (const module of rust().modulesAt(file, match.index)) {
        const target = rust().resolve(module, path);
        if (target && manifest.test(target.file))
          list(file, target.file, inTest(file, match.index));
      }
    }
  }
  assert(
    [...listing].some(([file, { all }]) => file.startsWith('app/') && all.size > 1),
    'the app lists the features and collectors',
  );
  holds(
    'encapsulation',
    'lists',
    [...listing]
      .filter(([file, found]) => listsOthers(file, found))
      .map(([file, { all }]) => `${file} lists ${[...all].sort().join(', ')}`),
  );
});

test("a feature's or collector's tests list only itself and what it may use", () => {
  const listing = (product: string[], all: string[]) => ({
    product: new Set(product),
    all: new Set(all),
  });
  // DVIC keeps a Cortex collection, so its tests may install a registry of both.
  const dvic = 'features/dvic/tests/backend/integration/dvic.rs';
  assert(!listsOthers(dvic, listing([], ['collectors/cortex', 'features/dvic'])));
  // An owner it does not declare is still caught.
  assert(listsOthers(dvic, listing([], ['collectors/paycom', 'features/dvic'])));
  assert(listsOthers(dvic, listing([], ['collectors/cortex', 'features/dvic', 'features/routes'])));
  // A collector's tests list no feature, core's tests nothing, and product code is held as before.
  assert(
    listsOthers(
      'collectors/cortex/tests/backend/a.rs',
      listing([], ['collectors/cortex', 'features/dvic']),
    ),
  );
  assert(
    listsOthers(
      'core/collection/tests/backend/a.rs',
      listing([], ['collectors/cortex', 'collectors/paycom']),
    ),
  );
  assert(
    listsOthers(
      'features/dvic/backend/a.rs',
      listing(['collectors/cortex', 'features/dvic'], ['collectors/cortex', 'features/dvic']),
    ),
  );
  assert(
    !listsOthers(
      'app/backend/lib.rs',
      listing(['collectors/cortex', 'features/dvic'], ['collectors/cortex', 'features/dvic']),
    ),
  );
});
