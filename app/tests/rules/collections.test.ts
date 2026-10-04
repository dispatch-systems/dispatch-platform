import test from 'node:test';
import { localDependencies, readCrate } from './support/cargo.js';
import { collections, keepers, kept } from './support/manifests.js';
import { holds } from './support/holds.js';
import { features, files, isFile, ownerOf, type Owner } from './support/repo.js';
import { rust } from './support/rust.js';

// Collectors gather, features keep: each collection's data goes to the one feature that keeps
// it. plans/restructure/structure.md, "How a collection runs".

/** The collectors a feature uses: its Cargo.toml's, or until crates exist, those its Rust names. */
function uses(owner: Owner): Set<string> {
  const manifest = `${owner.dir}/Cargo.toml`;
  if (isFile(manifest))
    return new Set(
      localDependencies(readCrate(manifest), rust().crates).filter((dir) =>
        dir.startsWith('collectors/'),
      ),
    );
  const named = new Set<string>();
  for (const file of files.filter(
    (file) => file.startsWith(`${owner.dir}/`) && rust().mounts.has(file),
  ))
    for (const reference of rust().references(file)) {
      const target = ownerOf(reference.to);
      if (target?.layer === 'collector') named.add(target.dir);
    }
  return named;
}

test('every collection has exactly one keeper, and every keeper keeps a collection', () => {
  const offered = collections();
  const keeping = features().flatMap((owner) =>
    kept(owner).map((keeper) => ({ owner, ...keeper })),
  );
  const wrong: string[] = [];
  for (const collection of offered) {
    const by = keeping.filter((keeper) => keeper.jobKind === collection.jobKind);
    if (by.length !== 1)
      wrong.push(`${collection.owner.dir}'s ${collection.jobKind} has ${by.length} keepers`);
  }
  for (const keeper of keeping)
    if (!offered.some((collection) => collection.jobKind === keeper.jobKind))
      wrong.push(
        `${keeper.at} keeps ${keeper.jobKind ?? 'nothing it names'}, which no collector collects`,
      );
  // A keeper no manifest lists keeps nothing, and only a feature keeps.
  for (const keeper of keepers())
    if (
      keeper.owner.layer !== 'feature' ||
      !kept(keeper.owner).some(({ at }) => at === `${keeper.owner.dir}: ${keeper.type}`)
    )
      wrong.push(`${keeper.file}: ${keeper.type} is a keeper no feature's manifest lists`);
  if (!offered.length) wrong.push('no collector declares a collection');
  holds('collections', 'keepers', wrong);
});

test('a feature keeps only collections of the collectors it uses', () => {
  const wrong: string[] = [];
  for (const owner of features())
    for (const keeper of kept(owner)) {
      const collection = collections().find((c) => c.jobKind === keeper.jobKind);
      if (collection && !uses(owner).has(collection.owner.dir))
        wrong.push(
          `${keeper.at} keeps ${collection.jobKind} from ${collection.owner.dir}, which it does not use`,
        );
    }
  holds('collections', 'collectors used', wrong);
});
