import assert from 'node:assert/strict';
import test from 'node:test';
import { localDependencies, readCrate, type Crate } from './support/cargo.js';
import { declared, inTest } from './support/manifests.js';
import { holds } from './support/holds.js';
import {
  collectors,
  features,
  files,
  isFile,
  ownerOf,
  unitOf,
  type Owner,
} from './support/repo.js';
import { lex, rust, useTree } from './support/rust.js';

// Dependencies point one way, app → features → collectors → core, and are declared: a
// feature's manifest names the features and collectors it uses, its Cargo.toml lists the
// same, and its Rust reaches nothing else. plans/restructure/enforcement.md, sections 1 and 3.
// What a feature's TypeScript may import is encapsulation.test.ts's rule, from the same list.

/** What an owner may use: core and itself always, and a feature what it declares. */
function allowed(owner: Owner): Set<string> {
  return new Set(['core', unitOf(owner), ...declared(owner)]);
}
/** The owner or other top folder a file belongs to, as a dependency names it. */
const unitOfFile = (file: string) => {
  const owner = ownerOf(file);
  return owner ? unitOf(owner) : file.split('/').slice(0, 2).join('/');
};

/** The owners of the workspace crates a Cargo.toml lists. */
const workspaceDependencies = (crate: Crate, dev: boolean) =>
  localDependencies(crate, rust().crates, dev).map((dir) => unitOfFile(`${dir}/Cargo.toml`));

test("each feature's and collector's Cargo.toml lists exactly what its manifest declares", () => {
  const disagree: string[] = [];
  for (const owner of [...features(), ...collectors()]) {
    const manifest = `${owner.dir}/Cargo.toml`;
    if (!isFile(manifest)) continue;
    const crate = readCrate(manifest);
    const listed = new Set(workspaceDependencies(crate, false));
    const expected = new Set(['core', ...declared(owner)]);
    for (const dir of listed)
      if (!expected.has(dir))
        disagree.push(`${manifest} lists ${dir}, which its manifest does not declare`);
    for (const dir of expected)
      if (!listed.has(dir)) disagree.push(`${manifest} lacks ${dir}, which its manifest declares`);
    // Its tests may use what it may: core's test helpers, its collectors and features.
    for (const dir of workspaceDependencies(crate, true))
      if (!expected.has(dir)) disagree.push(`${manifest}'s dev-dependencies list ${dir}`);
  }
  holds('dependencies', 'cargo', disagree);
});

test("core's Cargo.toml lists none of our crates", () => {
  const listed = isFile('core/Cargo.toml')
    ? [true, false].flatMap((dev) => workspaceDependencies(readCrate('core/Cargo.toml'), dev))
    : [];
  assert.deepEqual(
    listed.filter((dir) => dir !== 'core'),
    [],
  );
});

test("a feature's manifest declares only features and collectors that exist", () => {
  for (const owner of features())
    for (const dir of declared(owner))
      assert(
        isFile(`${dir}/feature.rs`) || isFile(`${dir}/collector.rs`),
        `${owner.dir} declares ${dir}, which is no feature or collector`,
      );
});

// Until each owner is its own crate, the compiler cannot hold this; the module tree can.
test("each owner's Rust names only core, itself and what it declares", () => {
  const reaches = new Set<string>();
  for (const file of rust().mounts.keys()) {
    const owner = ownerOf(file);
    if (!owner || owner.layer === 'app') continue;
    const may = allowed(owner);
    for (const reference of rust().references(file)) {
      const target = unitOfFile(reference.to);
      // Named by the part or owner that does it, its tests apart: different steps end them.
      const from = inTest(file, reference.offset)
        ? `${owner.dir}'s tests name`
        : `${owner.dir} names`;
      if (!may.has(target)) reaches.add(`${from} ${target}`);
    }
  }
  holds('dependencies', 'rust', reaches);
});

test('the module tree follows use trees, re-exports, globs and #[path] mounts', () => {
  assert.deepEqual(useTree('crate::{a, b::{c as d, *}, e::self}'), [
    { segments: ['crate', 'a'], glob: false },
    { segments: ['crate', 'b', 'c'], alias: 'd', glob: false },
    { segments: ['crate', 'b'], glob: true },
    { segments: ['crate', 'e'], glob: false },
  ]);
  const source = lex('let s = "crate::x"; // crate::y\n/* use crate::z; */ let c = \'"\';');
  assert(!/crate::/.test(source.masked), 'comments and strings are not code');
  assert.deepEqual(
    source.literals.map(({ value }) => value),
    ['crate::x'],
  );
  // The real tree: a crate the app depends on, and re-exports at its root, lead to the
  // module that defines the item.
  const lib = rust().roots.find(
    (root) => root.kind === 'lib' && files.includes(root.file) && root.file.startsWith('app/'),
  );
  if (lib) {
    const state = rust().resolve(lib.module, ['dispatch_core', 'State']);
    assert(state && ownerOf(state.file)?.layer === 'core', 'State is core');
  }
});
