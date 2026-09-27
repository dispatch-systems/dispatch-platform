import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fixture } from '../support/support.js';

test('only an authenticated platform owner receives a short-lived playground ticket', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-playground-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const key = 'b'.repeat(64);
  const keyFile = path.join(directory, 'key');
  fs.writeFileSync(keyFile, key, { mode: 0o600 });
  const f = await fixture({
    env: {
      DISPATCH_PLAYGROUND_ORIGIN: 'https://playground.example.com',
      DISPATCH_PLAYGROUND_KEY_FILE: keyFile,
    },
  });
  t.after(f.close);
  const route = '/api/platform/design-playground';
  assert.equal((await f.request(route)).status, 401);
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get(route)).status, 403);
  const owner = await f.client();
  const first = await owner.get(route);
  assert.equal(first.status, 200);
  assert.equal(first.value.origin, 'https://playground.example.com');
  const [encoded, signature] = first.value.ticket.split('.');
  assert.equal(signature, createHmac('sha256', key).update(encoded).digest('base64url'));
  const claims = JSON.parse(Buffer.from(encoded, 'base64url').toString());
  assert.equal(claims.aud, first.value.origin);
  assert.equal(claims.iss, f.env.DISPATCH_ORIGIN);
  assert.equal(claims.exp, first.value.expiresAt);
  assert(claims.exp > Date.now() && claims.exp <= Date.now() + 30000);
  assert(!JSON.stringify(claims).includes(owner.session.user.email));
  const second = await owner.get(route);
  assert.notEqual(first.value.ticket, second.value.ticket);
});

test('unconfigured playground returns no access material', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  assert.deepEqual((await owner.get('/api/platform/design-playground')).value, {
    origin: null,
    ticket: null,
    expiresAt: null,
  });
});
