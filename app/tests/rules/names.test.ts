import assert from 'node:assert/strict';
import test from 'node:test';
import ts from 'typescript';
import { readCrate } from './support/cargo.js';
import { collectorManifest, featureManifest } from './support/manifests.js';
import { holds, pendingNames } from './support/pending.js';
import { collectors, features, files, isFile, owners, read, templated } from './support/repo.js';
import { lexFile, modDeclarations, rust } from './support/rust.js';

/** A name's words, at separators and CamelCase: `RouteDataSettings` is route, data, settings. */
function words(name: string): string[] {
  return (
    name
      // An acronym spelled in mixed case is one word.
      .replace(/OAuth/g, 'Oauth')
      .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
      .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
      .split(/[\s._-]+/)
      .filter(Boolean)
      .map((word) => word.toLowerCase())
  );
}

// The names the restructure retired, and where each may still stand.
const retired: { name: string; allowed: (at: string) => boolean }[] = [
  { name: 'workforce', allowed: () => false },
  { name: 'routedata', allowed: () => false },
  // Timecard's meal breaks, and the meal data a collector reads from its site.
  { name: 'meals', allowed: (at) => /^(features\/timecard|collectors)\//.test(at) },
  { name: 'drivers', allowed: () => false },
  // Driver Match's directory and modules are driver_match; its words may name components.
  { name: 'driver-match', allowed: () => false },
  // A collector's page script that signs in to its site.
  { name: 'auth', allowed: (at) => /^collectors\/[^/]+\/scripts\//.test(at) },
  { name: 'browsers', allowed: () => false },
  { name: 'collectors', allowed: (at) => at === 'collectors' },
  { name: 'agents', allowed: (at) => at.startsWith('core/platform_owner/') },
];
/** The retired names a name uses: as a whole word or words, or spelled as it was. */
function retiredIn(name: string): string[] {
  const parts = words(name);
  return retired
    .map((entry) => entry.name)
    .filter((old) =>
      old.includes('-')
        ? name.toLowerCase().includes(old)
        : parts.some((_, start) =>
            parts.some((__, end) => end >= start && parts.slice(start, end + 1).join('') === old),
          ),
    );
}
const allowedAt = (old: string, at: string) => retired.find((r) => r.name === old)!.allowed(at);
/**
 * A current feature's or collector's name in kebab case, as TypeScript and CSS files spell
 * names: `driver-match.spec.ts`. As a folder or module it stays retired.
 */
const kebab = new Set(
  [...features(), ...collectors()].map(({ name }) => name.replaceAll('_', '-')),
);
const scriptOrStyle = (at: string) => /\.(tsx?|css)$/.test(at);

// A database keeps its name on disk and in its migrations' folders, so it may be named so.
const databaseName = (at: string) =>
  /(^|\/)migrations\/[^/]+(\/|$)/.test(at) || /^core\/db\/tests\/backend\/schema\//.test(at);

test('a retired name comes back as no module, file or folder', () => {
  const found: string[] = [];
  const check = (name: string, at: string, what: string) => {
    if (databaseName(at)) return;
    for (const old of retiredIn(name))
      if (!allowedAt(old, at) && !(what === 'file' && scriptOrStyle(at) && kebab.has(old)))
        found.push(`${what} ${at} is named ${old}`);
  };
  const folders = new Set<string>();
  for (const file of files.map(templated)) {
    const parts = file.split('/');
    parts.slice(0, -1).forEach((_, index) => folders.add(parts.slice(0, index + 1).join('/')));
    // A file's name without its extensions: `api-workforce.test.ts` is api-workforce.
    check(parts.at(-1)!.replace(/\..*$/, ''), file, 'file');
  }
  for (const folder of folders) check(folder.split('/').at(-1)!, folder, 'folder');
  for (const file of rust().mounts.keys())
    for (const declaration of modDeclarations(lexFile(file)))
      check(declaration.name, file, `module ${declaration.name} in`);
  holds('names', 'retired names', found);
});

test('the retired names are found inside CamelCase and kebab-case names', () => {
  assert.deepEqual(retiredIn('RouteDataSettings'), ['routedata']);
  assert.deepEqual(retiredIn('api-workforce'), ['workforce']);
  assert.deepEqual(retiredIn('driver-match'), ['driver-match']);
  assert.deepEqual(retiredIn('driver_match'), []);
  assert.deepEqual(retiredIn('DriverMatchTabLabel'), []);
  assert.deepEqual(retiredIn('AuthorizePage'), []);
  assert.deepEqual(retiredIn('oauth'), []);
  assert.deepEqual(retiredIn('OAuthAllowedApp'), []);
  assert.deepEqual(retiredIn('AuthScreen'), ['auth']);
  assert.deepEqual(retiredIn('useAgents'), ['agents']);
  assert.deepEqual(retiredIn('DriverSheet'), []);
});

// The `name` its frontend manifest gives, read from the source.
function frontendName(file: string): string | undefined {
  const source = ts.createSourceFile(file, read(file), ts.ScriptTarget.Latest, true);
  let name: string | undefined;
  const visit = (node: ts.Node) => {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.name.text === 'feature' &&
      node.initializer &&
      ts.isObjectLiteralExpression(node.initializer)
    )
      for (const property of node.initializer.properties)
        if (
          ts.isPropertyAssignment(property) &&
          ts.isIdentifier(property.name) &&
          property.name.text === 'name' &&
          ts.isStringLiteralLike(property.initializer)
        )
          name = property.initializer.text;
    ts.forEachChild(node, visit);
  };
  visit(source);
  return name;
}

test("a directory's name is the name its manifests give", () => {
  const differ: string[] = [];
  const same = (dir: string, what: string, name: string | undefined, expected: string) => {
    if (name !== undefined && name !== expected) differ.push(`${dir}: ${what} says ${name}`);
  };
  for (const owner of features()) {
    same(owner.dir, 'feature.rs', featureManifest(owner)?.name, owner.name);
    if (isFile(`${owner.dir}/feature.rs`) && !featureManifest(owner)?.name)
      differ.push(`${owner.dir}: feature.rs gives no name`);
  }
  for (const owner of collectors())
    same(owner.dir, 'collector.rs', collectorManifest(owner)?.id, owner.name);
  for (const owner of owners().filter((owner) => owner.layer !== 'app')) {
    const manifest = `${owner.dir}/frontend/feature.ts`;
    if (isFile(manifest))
      same(owner.dir, 'frontend/feature.ts', frontendName(manifest), owner.name);
  }
  // Each crate is named after its directory: dispatch-<name>, with dashes for underscores.
  for (const owner of [...features(), ...collectors()]) {
    const manifest = `${owner.dir}/Cargo.toml`;
    if (isFile(manifest))
      same(
        owner.dir,
        'Cargo.toml',
        readCrate(manifest).name,
        `dispatch-${owner.name.replaceAll('_', '-')}`,
      );
  }
  if (isFile('core/Cargo.toml'))
    same('core', 'Cargo.toml', readCrate('core/Cargo.toml').name, 'dispatch-core');
  holds('names', 'manifest names', differ);
});

test('pending.json names only these checks', () => {
  pendingNames('names', ['retired names', 'manifest names']);
});
