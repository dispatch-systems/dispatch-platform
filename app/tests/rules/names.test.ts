import test from 'node:test';
import { holds } from './support/holds.js';
import { collectorManifests, featureManifests } from './support/manifests.js';
import { files, isTestFile, ownerOf, read } from './support/repo.js';
import { lexFile } from './support/rust.js';

// Core names no feature or collector: what a feature adds reaches core through a slot, and a
// build that leaves one out leaves core as it was. Its product code holds no permission a
// feature declares and no collector's id as a string; its tests may name them, and its docs
// may use them as examples.
const coreProduct = (file: string) =>
  ownerOf(file)?.layer === 'core' &&
  !isTestFile(file) &&
  !file.includes('/generated/') &&
  /\.(rs|ts|tsx)$/.test(file);

/** The strings `file`'s code holds, outside its comments. */
function strings(file: string): string[] {
  if (file.endsWith('.rs')) return lexFile(file).literals.map((literal) => literal.value);
  const code = read(file).replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, '');
  return [...code.matchAll(/(['"`])([^'"`\n]*)\1/g)].map((match) => match[2]!);
}

function named(names: Map<string, string>): string[] {
  return files.filter(coreProduct).flatMap((file) =>
    strings(file)
      .filter((value) => names.has(value))
      .map((value) => `${file} names ${names.get(value)} "${value}"`),
  );
}

test("core's product code names no feature's permission", () => {
  const permissions = new Map(
    featureManifests().flatMap(({ owner, permissions }) =>
      permissions.map((id) => [id, `${owner.dir}'s permission`] as const),
    ),
  );
  holds('names', 'permissions', named(permissions));
});

test("core's product code names no collector", () => {
  const collectors = new Map(
    collectorManifests().flatMap(({ owner, id }) => (id ? [[id, owner.dir] as const] : [])),
  );
  holds('names', 'collectors', named(collectors));
});
