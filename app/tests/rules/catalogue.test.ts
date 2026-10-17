import test from 'node:test';
import { holds } from './support/holds.js';
import { entries, read } from './support/repo.js';
import { lexFile } from './support/rust.js';

// core/manifest/README.md lists every slot a feature fills, backend and frontend, so a reader
// finds each where it is declared. A slot added without its row fails here.

const CATALOGUE = 'core/manifest/README.md';

/** The fields of `Feature`, as `core/manifest/backend/mod.rs` declares them. */
function backendSlots(): string[] {
  const { masked } = lexFile('core/manifest/backend/mod.rs');
  const open = masked.indexOf('pub struct Feature {');
  const close = masked.indexOf('\n}', open);
  return [...masked.slice(open, close).matchAll(/^\s+pub ([a-z_]+):/gm)].map((match) => match[1]!);
}

/** The fields of `FrontendFeature`, as each slot's module adds them. */
function frontendSlots(): string[] {
  const folder = 'core/shell/frontend/runtime/slots';
  return entries(folder).flatMap((name) =>
    [...read(`${folder}/${name}`).matchAll(/interface FrontendFeature \{([^}]*)\}/g)].flatMap(
      ([, fields]) => [...fields!.matchAll(/^\s+([a-zA-Z]+)\??:/gm)].map((match) => match[1]!),
    ),
  );
}

test('the manifest README lists every backend and frontend slot', () => {
  const catalogue = read(CATALOGUE);
  const listed = (slot: string) => catalogue.includes(`| \`${slot}\``);
  holds(
    'catalogue',
    'slots',
    [
      ...backendSlots().map((slot) => ['Feature', slot]),
      ...frontendSlots().map((slot) => ['FrontendFeature', slot]),
    ]
      .filter(([, slot]) => !listed(slot!))
      .map(([type, slot]) => `${type}.${slot} has no row in ${CATALOGUE}`),
  );
});
