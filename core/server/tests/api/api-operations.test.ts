import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fixture } from '../../../shell/tests/support/support.js';

// What a restore ends of the MCP's, its unspent sign-in codes, is the MCP's own test, in
// mcp/tests/api/backups.test.ts.
test('Rust backup and restore validate checksums, exclude runtime locks and revoke web capabilities', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const backup = `${f.root}-backup`,
    restored = `${f.root}-restored`;
  t.after(() => {
    fs.rmSync(backup, { recursive: true, force: true });
    fs.rmSync(restored, { recursive: true, force: true });
  });
  await f.stop();
  f.cli(['backup', backup]);
  f.cli(['restore', backup, restored]);
  const db = new DatabaseSync(path.join(restored, 'data/platform/accounts.sqlite'));
  assert.equal((db.prepare('SELECT count(*) n FROM sessions').get() as { n: number }).n, 0);
  db.close();
  const manifest = JSON.parse(fs.readFileSync(path.join(backup, 'backup.json'), 'utf8'));
  assert(!manifest.files.some((file: { path: string }) => file.path.endsWith('.lock')));
  const providerFiles = manifest.files.filter((file: { path: string }) =>
    file.path.endsWith('/data/paycom/paycom.sqlite'),
  );
  assert.equal(providerFiles.length, 3);
  for (const file of providerFiles) {
    const provider = new DatabaseSync(path.join(restored, file.path), { readOnly: true });
    assert.equal(provider.prepare('PRAGMA integrity_check').get()!.integrity_check, 'ok');
    assert.equal(
      provider.prepare('SELECT provider FROM storage_identity').get()!.provider,
      'paycom',
    );
    provider.close();
  }

  fs.appendFileSync(path.join(backup, manifest.files[0].path), 'tampered');
  fs.rmSync(restored, { recursive: true });
  assert.throws(() => f.cli(['restore', backup, restored]), /backup_checksum_failed/);
  assert.equal(fs.existsSync(restored), false);
  await f.start();
  assert.equal((await owner.get('/api/session')).status, 200);
});

test('SQLite write contention leaves health responsive and the pending write recovers after unlock', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  // The server may still be finishing a write, such as the sign-in's, when BEGIN IMMEDIATE runs.
  const db = new DatabaseSync(path.join(f.root, 'data/platform/accounts.sqlite'), {
    timeout: 5000,
  });
  let pending: Promise<unknown> | undefined;
  try {
    db.exec('BEGIN IMMEDIATE');
    pending = owner.post('/api/platform/dsps', { ownerEmail: 'blocked@dispatch.test' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    const started = Date.now();
    assert.equal((await f.request('/api/health')).status, 200);
    assert(Date.now() - started < 500);
    db.exec('ROLLBACK');
    assert.equal(((await pending) as { status: number }).status, 201);
  } finally {
    if (db.isTransaction) db.exec('ROLLBACK');
    db.close();
    await pending;
  }
});
