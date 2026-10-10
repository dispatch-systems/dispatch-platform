import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fixture } from '../../../core/shell/tests/support/support.js';

// What a restored backup keeps of the MCP's: a spent sign-in code stays, an unspent one, a
// short-lived capability that could still mint access, ends.
test('a restore ends the sign-in codes no app has redeemed yet', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const backup = `${f.root}-backup`,
    restored = `${f.root}-restored`;
  t.after(() => {
    fs.rmSync(backup, { recursive: true, force: true });
    fs.rmSync(restored, { recursive: true, force: true });
  });
  await f.stop();
  const source = new DatabaseSync(path.join(f.root, 'data/platform/accounts.sqlite'));
  source.exec(`
    INSERT INTO oauth_codes(
      hash,client_id,client_name,client_verified,redirect_uri,code_challenge,resource,choices,
      approved_by,created_at,expires_at,used_at
    ) VALUES
      ('unused-approval','client','Client',0,'http://127.0.0.1/callback','challenge',
       'https://dispatch.test/api','{}','owner',datetime('now'),datetime('now','+5 minutes'),NULL),
      ('used-approval','client','Client',0,'http://127.0.0.1/callback','challenge',
       'https://dispatch.test/api','{}','owner',datetime('now'),datetime('now','+5 minutes'),datetime('now'));
  `);
  source.close();
  f.cli(['backup', backup]);
  f.cli(['restore', backup, restored]);
  const db = new DatabaseSync(path.join(restored, 'data/platform/accounts.sqlite'));
  const codes = (hash: string) =>
    (db.prepare('SELECT count(*) n FROM oauth_codes WHERE hash=?').get(hash) as { n: number }).n;
  assert.equal(codes('unused-approval'), 0);
  assert.equal(codes('used-approval'), 1);
  db.close();
});

test('restore accepts a backup from before OAuth tables existed', async (t) => {
  const f = await fixture(false);
  t.after(f.close);
  const backup = `${f.root}-pre-oauth-backup`,
    restored = `${f.root}-pre-oauth-restored`;
  t.after(() => {
    fs.rmSync(backup, { recursive: true, force: true });
    fs.rmSync(restored, { recursive: true, force: true });
  });
  await f.stop();
  const db = new DatabaseSync(path.join(f.root, 'data/platform/accounts.sqlite'));
  db.exec(
    'DROP TABLE oauth_tokens; DROP TABLE oauth_codes; DROP TABLE oauth_requests; DROP TABLE oauth_clients;',
  );
  db.close();
  f.cli(['backup', backup]);
  f.cli(['restore', backup, restored]);
  assert(fs.existsSync(path.join(restored, 'data/platform/accounts.sqlite')));
});
