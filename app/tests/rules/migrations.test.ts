import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { migrations, type Migration } from './support/manifests.js';
import { holds, pendingNames } from './support/pending.js';
import { files, ownerOf, read, root, unitOf } from './support/repo.js';

// Every shipped migration, per database: its number, name, and a digest of what it applies:
// its SQL, or the name of the function a `Code` migration runs with the SQL files that
// function includes. A database's record of the migrations it ran is keyed by number for
// good, so a shipped one never changes. New ones are appended: record them with
// `DISPATCH_RECORD_MIGRATIONS=1 npx tsx --test app/tests/rules/migrations.test.ts`.
const historyFile = path.join(import.meta.dirname, 'migrations-history.json');
type Entry = { id: number; name: string; sha256: string };
type History = Record<string, Entry[]>;

const sha256 = (text: string) => createHash('sha256').update(text).digest('hex');
/** A migration's file, as its database's folder and its own name: `paycom/0002_….sql`. */
const sqlName = (file: string) => file.split('/').slice(-2).join('/');
function entry(migration: Migration): Entry {
  const applied = migration.code
    ? [`fn ${migration.code}`, ...migration.includes.map((file) => read(file))].join('\n')
    : (migration.sql?.text ?? '');
  return { id: migration.id, name: migration.name ?? '', sha256: sha256(applied) };
}

const gathered = migrations();
const where = (migration: Migration) =>
  `${migration.owner.dir}'s ${migration.database} migration ${migration.id}`;

test("each database's gathered migrations run from 1 without a gap or a repeat", () => {
  const wrong: string[] = [];
  const databases = new Map<string, Migration[]>();
  for (const migration of gathered) {
    if (!migration.database) wrong.push(`${where(migration)} names no database`);
    if (!migration.name) wrong.push(`${where(migration)} has no name`);
    if (!migration.sql && !migration.code)
      wrong.push(`${where(migration)} applies nothing it names`);
    databases.set(migration.database ?? '', [
      ...(databases.get(migration.database ?? '') ?? []),
      migration,
    ]);
  }
  for (const [database, list] of databases) {
    const ids = list.map(({ id }) => id).sort((a, b) => a - b);
    ids.forEach((id, index) => {
      if (id === ids[index - 1]) wrong.push(`${database} migration ${id} is declared twice`);
    });
    const unique = [...new Set(ids)];
    unique.forEach((id, index) => {
      if (id !== index + 1) wrong.push(`${database} migrations skip ${index + 1}`);
    });
  }
  assert(gathered.length > 30, `found only ${gathered.length} migrations`);
  holds('migrations', 'ledger', [...new Set(wrong)]);
});

// plans/restructure/structure.md, "Data": a migration lives with its owner, in a file that
// names its database, number and name.
test('each migration file lives with its owner and is applied by exactly the migration it names', () => {
  const wrong: string[] = [];
  const applied = new Map<string, Migration[]>();
  for (const migration of gathered) {
    const sql = migration.sql?.file ? [migration.sql.file] : [];
    for (const file of [...sql, ...migration.includes]) {
      applied.set(file, [...(applied.get(file) ?? []), migration]);
      const expected = `${migration.database}/${String(migration.id).padStart(4, '0')}_${migration.name}.sql`;
      if (sqlName(file) !== expected || !/\/migrations\/[^/]+\/[^/]+\.sql$/.test(file))
        wrong.push(`${where(migration)} applies ${file}, not migrations/${expected}`);
      const owner = ownerOf(file);
      if (!owner || unitOf(owner) !== unitOf(migration.owner))
        wrong.push(`${where(migration)} applies ${file}, which another owner holds`);
    }
  }
  for (const file of files.filter((file) => /(^|\/)migrations\/[^/]+\/[^/]+\.sql$/.test(file))) {
    const by = applied.get(file) ?? [];
    if (by.length !== 1) wrong.push(`${file} is applied by ${by.length} migrations`);
  }
  holds('migrations', 'files', wrong);
});

test('a shipped migration keeps its number, name and contents, and new ones are appended', () => {
  const history: History = JSON.parse(fs.readFileSync(historyFile, 'utf8'));
  const changed: string[] = [];
  const current = new Map(
    gathered.map((migration) => [`${migration.database}:${migration.id}`, migration]),
  );
  for (const [database, entries] of Object.entries(history)) {
    entries.forEach((recorded, index) => {
      assert.equal(recorded.id, index + 1, `the ${database} history must run from 1 without a gap`);
      const migration = current.get(`${database}:${recorded.id}`);
      if (!migration) return changed.push(`${database} migration ${recorded.id} is gone`);
      assert.deepEqual(entry(migration), recorded, `${database} migration ${recorded.id} changed`);
    });
  }
  const recordedIds = (database: string) => (history[database] ?? []).map(({ id }) => id);
  const added = gathered
    .filter((migration) => !recordedIds(migration.database!).includes(migration.id))
    .sort((a, b) => a.id - b.id);
  for (const migration of added) {
    const last = Math.max(0, ...recordedIds(migration.database!));
    if (migration.id <= last)
      changed.push(`${where(migration)} sits among shipped migrations instead of after them`);
  }
  assert.deepEqual(changed, []);
  if (process.env.DISPATCH_RECORD_MIGRATIONS === '1' && added.length) {
    for (const migration of added) (history[migration.database!] ??= []).push(entry(migration));
    fs.writeFileSync(historyFile, `${JSON.stringify(history, null, 2)}\n`);
    process.stdout.write(
      `Recorded ${added.length} migrations in ${path.relative(root, historyFile)}.\n`,
    );
  }
});

test('the history records every database the migrations name', () => {
  const history: History = JSON.parse(fs.readFileSync(historyFile, 'utf8'));
  const databases = new Set(gathered.map((migration) => migration.database));
  for (const database of Object.keys(history))
    assert(databases.has(database), `the history records ${database}, which no migration names`);
});

test('pending.json names only these checks', () => {
  pendingNames('migrations', ['ledger', 'files']);
});
