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

// Dependencies point one way, app → mcp → features → collectors → core, and are declared: a
// feature's manifest names the features and collectors it uses, its Cargo.toml lists the
// same, and its Rust reaches nothing else.
// What a feature's TypeScript may import is encapsulation.test.ts's rule, from the same list.

/**
 * A feature's contract crate, `features/<name>/contract/`: the trait it exposes and the types
 * that trait speaks in, on core alone. Any owner may use one without declaring its feature, as
 * it stays when the feature is left out of a build.
 */
const contract = (file: string) =>
  /^features\/[^/]+\/contract(\/|$)/.exec(file)?.[0].replace(/\/$/, '');
/** What an owner may use: core, itself and every contract always, and a feature what it declares. */
function allowed(owner: Owner): Set<string> {
  const contracts = files.flatMap((file) => contract(file) ?? []);
  return new Set(['core', unitOf(owner), ...contracts, ...declared(owner)]);
}
/** The owner or other top folder a file belongs to, as a dependency names it: a contract apart. */
const unitOfFile = (file: string) => {
  const owner = ownerOf(file);
  return contract(file) ?? (owner ? unitOf(owner) : file.split('/').slice(0, 2).join('/'));
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
      if (!expected.has(dir) && !contract(dir))
        disagree.push(`${manifest} lists ${dir}, which its manifest does not declare`);
    for (const dir of expected)
      if (!listed.has(dir)) disagree.push(`${manifest} lacks ${dir}, which its manifest declares`);
    // Its tests may use what it may: core's test helpers, its collectors and features.
    for (const dir of workspaceDependencies(crate, true))
      if (!expected.has(dir) && !contract(dir))
        disagree.push(`${manifest}'s dev-dependencies list ${dir}`);
  }
  holds('dependencies', 'cargo', disagree);
});

test("a contract's Cargo.toml lists core alone, and its feature's lists the contract", () => {
  const wrong: string[] = [];
  for (const dir of new Set(files.flatMap((file) => contract(file) ?? []))) {
    const crate = readCrate(`${dir}/Cargo.toml`);
    for (const dev of [false, true])
      for (const unit of workspaceDependencies(crate, dev))
        if (unit !== 'core' && unit !== dir)
          wrong.push(`${dir}/Cargo.toml${dev ? "'s dev-dependencies" : ''} lists ${unit}`);
    const feature = dir.replace(/\/contract$/, '');
    if (!workspaceDependencies(readCrate(`${feature}/Cargo.toml`), false).includes(dir))
      wrong.push(`${feature}/Cargo.toml lacks its own contract, ${dir}`);
  }
  assert.deepEqual(wrong, []);
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

// The MCP sits above the features. It uses core, and a feature's crate only as the features its
// tools need bring it: an optional dependency that the MCP's Cargo feature of that feature turns
// on, with the crates the MCP's tests install beside it, as `dispatchdev generate` writes
// them. Its tests use nothing else.
test("the MCP's Cargo.toml lists core, and features and collectors only as its tools bring them", () => {
  if (!isFile('mcp/Cargo.toml')) return;
  const crate = readCrate('mcp/Cargo.toml');
  const ours = (dev: boolean) =>
    crate.dependencies
      .filter((dependency) => dependency.dev === dev)
      .flatMap((dependency) => {
        const dir = localDependencies(
          { ...crate, dependencies: [dependency] },
          rust().crates,
          dev,
        )[0];
        return dir ? [{ dependency, unit: unitOfFile(`${dir}/Cargo.toml`) }] : [];
      });
  const wrong = [
    ...ours(false)
      .filter(({ unit }) => unit !== 'core')
      .flatMap(({ dependency, unit }) => [
        ...(/^(features|collectors)\//.test(unit) ? [] : [`mcp/Cargo.toml lists ${unit}`]),
        ...(dependency.optional ? [] : [`mcp/Cargo.toml lists ${unit} as always on`]),
      ]),
    ...ours(true)
      .filter(({ unit }) => unit !== 'core')
      .map(({ unit }) => `mcp/Cargo.toml's dev-dependencies list ${unit}`),
  ];
  assert(
    ours(false).some(({ unit }) => unit === 'core'),
    'the MCP depends on core',
  );
  holds('dependencies', 'the MCP', wrong);
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
    // A contract stays when its feature is left out, so it names core and itself alone.
    const own = contract(file);
    const may = own ? new Set(['core', own]) : allowed(owner);
    for (const reference of rust().references(file)) {
      const target = unitOfFile(reference.to);
      // Named by the part or owner that does it, its tests apart: different steps end them.
      const from = inTest(file, reference.offset)
        ? `${own ?? owner.dir}'s tests name`
        : `${own ?? owner.dir} names`;
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
