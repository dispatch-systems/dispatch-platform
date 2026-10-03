import assert from 'node:assert/strict';
import test from 'node:test';
import { inTest, productRust, stringsIn } from './support/manifests.js';
import { holds, pendingNames } from './support/pending.js';
import { files, ownerOf, read, unitOf } from './support/repo.js';
import { closing, lexFile, rust } from './support/rust.js';
import { createdTables, isSql, namedTables } from './support/sql.js';

// Owners keep their data: only a table's owner writes SQL against it. Core is one owner, so
// its parts share core's tables; a collector or feature reaches core's data through core's
// functions, never its SQL. plans/restructure/structure.md, "Data".
//
// A table's owner is the owner that declares it in its manifest (`tables: &[("paycom",
// &["employees", …])]`), or else the owner whose migrations create it. Shipped migrations
// keep their SQL for good, wherever it now belongs, so they claim tables but are not held
// to the rule. A name no owner creates, such as a test's deliberately missing table, is no
// one's table.

/** Tables two owners declare, which therefore have no one owner. */
const twice = new Set<string>();
/** The owner a file's SQL speaks for: core as a whole, a collector, a feature or the app. */
const speaker = (file: string) => unitOf(ownerOf(file)!);
/** Each table name with the owners that may name it, as `database/table` claims. */
function owners(): Map<string, Set<string>> {
  const declared = new Map<string, string>();
  const declare = (key: string, owner: string) => {
    if (declared.has(key) && declared.get(key) !== owner)
      twice.add(`${key} is declared by ${declared.get(key)} and ${owner}`);
    declared.set(key, owner);
  };
  for (const file of files.filter(productRust)) {
    const { masked } = lexFile(file);
    for (const field of masked.matchAll(/\btables\s*:\s*&\s*\[/g)) {
      if (inTest(file, field.index)) continue;
      const open = field.index + field[0].length - 1;
      const close = closing(masked, open);
      // Each `("database", &["table", …])`.
      for (const tuple of masked
        .slice(open, close)
        .matchAll(/\(\s*"[^"]*"\s*,\s*&\s*\[[^\]]*\]\s*\)/g)) {
        const start = open + tuple.index;
        const [database, ...names] = stringsIn({ file, start, end: start + tuple[0].length });
        for (const name of names) declare(`${database}/${name}`, speaker(file));
      }
    }
  }
  const created = new Map<string, Set<string>>();
  const claim = (key: string, owner: string) =>
    created.set(key, new Set([...(created.get(key) ?? []), owner]));
  for (const file of files.filter((file) => /(^|\/)migrations\/[^/]+\/[^/]+\.sql$/.test(file))) {
    const database = file.split('/').at(-2)!;
    for (const table of createdTables(read(file))) claim(`${database}/${table}`, speaker(file));
  }
  // Tables made in code: a `Code` migration's, or a test's own.
  for (const file of rustFiles())
    for (const literal of lexFile(file).literals)
      if (isSql(literal.value))
        for (const table of createdTables(literal.value)) claim(`?/${table}`, speaker(file));
  for (const [key, owner] of declared) created.set(key, new Set([owner]));
  const byName = new Map<string, Set<string>>();
  for (const [key, claims] of created) {
    const table = key.slice(key.indexOf('/') + 1);
    byName.set(table, new Set([...(byName.get(table) ?? []), ...claims]));
  }
  return byName;
}
/** The Rust of every owner but the app's whole-product tests, which cross owners by design. */
const rustFiles = () =>
  files.filter((file) => file.endsWith('.rs') && ownerOf(file) && !file.startsWith('app/tests/'));

test("each owner's SQL names only its own tables", () => {
  const tables = owners();
  const foreign = new Set<string>();
  let statements = 0;
  for (const file of rustFiles()) {
    if (!rust().mounts.has(file)) continue;
    const source = lexFile(file);
    const owner = speaker(file);
    for (const literal of source.literals) {
      if (!isSql(literal.value)) continue;
      statements++;
      for (const table of namedTables(literal.value)) {
        const claims = tables.get(table);
        if (!claims || claims.has(owner)) continue;
        const from = inTest(file, literal.start)
          ? `${ownerOf(file)!.dir}'s tests name`
          : `${ownerOf(file)!.dir} names`;
        // By owner, so the list reads as the steps that end each reach.
        foreign.add(`${from} tables of ${[...claims].sort().join(' or ')}`);
      }
    }
  }
  assert(statements > 300, `found only ${statements} SQL statements`);
  assert.deepEqual([...twice], [], 'every table has one owner');
  holds('ownership', 'tables', foreign);
});

test('SQL table names are read from every statement and clause', () => {
  assert.deepEqual(
    namedTables(
      'WITH recent AS (SELECT id FROM jobs) SELECT a.x FROM employees a, timecards t ' +
        'JOIN publications p ON p.id=a.id LEFT JOIN recent r ON r.id=a.id ' +
        "WHERE EXISTS (SELECT 1 FROM json_each(a.codes)) AND a.note='FROM users'",
    ),
    ['jobs', 'employees', 'timecards', 'publications'],
  );
  assert.deepEqual(namedTables('INSERT OR IGNORE INTO connections(provider) VALUES (?)'), [
    'connections',
  ]);
  assert.deepEqual(namedTables('UPDATE people SET name=? WHERE id=?'), ['people']);
  assert.deepEqual(
    namedTables('INSERT INTO a(x) VALUES (1) ON CONFLICT(x) DO UPDATE SET x=excluded.x'),
    ['a'],
  );
  assert.deepEqual(namedTables('DELETE FROM {table} WHERE dsp=?'), []);
  assert.deepEqual(namedTables('CREATE INDEX IF NOT EXISTS i ON audit(at)'), ['audit']);
  assert.deepEqual(createdTables('CREATE TABLE IF NOT EXISTS x(id); ALTER TABLE y RENAME TO z'), [
    'x',
    'z',
  ]);
  assert(!isSql('Select the DSP from the list'));
});

test('pending.json names only these checks', () => {
  pendingNames('ownership', ['tables']);
});
