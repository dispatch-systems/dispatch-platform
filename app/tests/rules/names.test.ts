import test from 'node:test';
import { holds } from './support/holds.js';
import { collectorManifests, declared, featureManifests } from './support/manifests.js';
import { files, isTestFile, ownerOf, read } from './support/repo.js';
import { lexFile } from './support/rust.js';

// Core names no feature or collector: what a feature adds reaches core through a slot, and a
// build that leaves one out leaves core as it was. Its product code holds no permission a
// feature declares and no collector's id as a string; its tests may name them, and its docs
// may use them as examples. Its `api/generated/` files are what `dispatchdev generate`
// writes from the registry, the one listing that names them all, so they are not its code.
const product = (file: string) =>
  !isTestFile(file) && !/\/api\/generated\//.test(file) && /\.(rs|ts|tsx)$/.test(file);
const coreProduct = (file: string) => ownerOf(file)?.layer === 'core' && product(file);

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

// Past core, each owner's product code names only its own permissions and those of the
// features it declares, and each owner's route listing only those its routes may: a build
// that leaves another feature out has no such permission, so the frontend no longer
// typechecks against the permissions it has, and the listing no longer matches the routes.
// The app lists every feature, so it may name any. Feature and page ids, words routes and
// tables use too, are left to the weekly removability check.
test("each owner's code and route listing name only its permissions and what it declares", () => {
  const owners = new Map(
    featureManifests().flatMap(({ owner, permissions }) =>
      permissions.map((id) => [id, owner.dir] as const),
    ),
  );
  const code = (file: string) =>
    !['core', 'app'].includes(ownerOf(file)?.layer ?? 'core') && product(file);
  const listing = (file: string) => /\/tests\/api\/routes\.txt$/.test(file);
  holds(
    'names',
    'permissions of what an owner declares',
    files
      .filter((file) => code(file) || listing(file))
      .flatMap((file) => {
        const owner = ownerOf(file);
        if (!owner) return [];
        const may = new Set([owner.dir, ...declared(owner)]);
        const values = listing(file)
          ? [...read(file).matchAll(/"([^"\n]*)"/g)].map((match) => match[1]!)
          : strings(file);
        return values
          .flatMap((value) => value.split('|'))
          .filter((value) => owners.has(value) && !may.has(owners.get(value)!))
          .map((value) => `${file} names ${owners.get(value)}'s permission "${value}"`);
      }),
  );
});

test("core's product code names no collector", () => {
  const collectors = new Map(
    collectorManifests().flatMap(({ owner, id }) => (id ? [[id, owner.dir] as const] : [])),
  );
  holds('names', 'collectors', named(collectors));
});
