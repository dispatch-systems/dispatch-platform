import assert from 'node:assert/strict';
import test from 'node:test';
import { holds, pendingNames } from './support/pending.js';
import { isTestFile, ownerOf, type Layer } from './support/repo.js';
import { lexFile, rust } from './support/rust.js';
import { imports, scriptFiles } from './support/typescript.js';

// Each owner's TypeScript reaches only what its layer may: a feature itself, core, and the
// features and collectors it declares (dependencies.test.ts holds which); a collector itself
// and core; core only core. Only the app imports features and collectors to list them.
// plans/restructure/enforcement.md, section 3; dashboard-structure.test.ts holds the front
// doors between frontends.
const reachable: Record<Exclude<Layer, 'app'>, Layer[]> = {
  feature: ['core', 'collector', 'feature'],
  collector: ['core'],
  core: ['core'],
};
/** The owner, or the top two folders, a file belongs to. */
const unitOf = (file: string) => ownerOf(file)?.dir ?? file.split('/').slice(0, 2).join('/');

test("each owner's TypeScript imports only what its layer may", () => {
  const reaches = new Set<string>();
  for (const file of scriptFiles()) {
    const owner = ownerOf(file);
    if (!owner || owner.layer === 'app') continue;
    const test = isTestFile(file);
    for (const { target } of imports(file)) {
      const to = ownerOf(target);
      if (to?.dir === owner.dir) continue;
      if (to && to.layer !== 'app' && reachable[owner.layer].includes(to.layer)) continue;
      // Tests may run on the tooling's harness and drive the services they exercise.
      if (test && /^(tooling|services)\//.test(target)) continue;
      reaches.add(`${owner.dir}${test ? "'s tests import" : ' imports'} ${unitOf(target)}`);
    }
  }
  holds('encapsulation', 'typescript', reaches);
});

// The frontend's list is app/frontend/features.ts; the backend's is the app's registry.
test('only the app lists the features and collectors', () => {
  const listing = new Map<string, Set<string>>();
  const list = (file: string, manifest: string) => {
    const owner = ownerOf(manifest);
    if (!owner || !['feature', 'collector'].includes(owner.layer)) return;
    listing.set(file, new Set([...(listing.get(file) ?? []), owner.dir]));
  };
  for (const file of scriptFiles())
    for (const { target } of imports(file))
      if (/^(features|collectors)\/[^/]+\/frontend\/feature\.ts$/.test(target)) list(file, target);
  const manifest = /^(features\/[^/]+\/feature|collectors\/[^/]+\/collector)\.rs$/;
  for (const file of rust().mounts.keys()) {
    // A manifest's file mounted as a module, or its `FEATURE` or `COLLECTOR` named.
    for (const reference of rust().references(file))
      if (reference.mount && manifest.test(reference.to)) list(file, reference.to);
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
        if (target && manifest.test(target.file)) list(file, target.file);
      }
    }
  }
  const lists = [...listing].filter(([, owners]) => owners.size > 1);
  assert(
    lists.some(([file]) => file.startsWith('app/')),
    'the app lists the features and collectors',
  );
  holds(
    'encapsulation',
    'lists',
    lists
      .filter(([file]) => !file.startsWith('app/'))
      .map(([file, owners]) => `${file} lists ${[...owners].sort().join(', ')}`),
  );
});

test('pending.json names only these checks', () => {
  pendingNames('encapsulation', ['typescript', 'lists']);
});
