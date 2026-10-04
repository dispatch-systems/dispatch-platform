import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { demo, fixture, until } from '../../../shell/tests/support/support.js';
const { password } = demo;

test('Rust password recovery uses the private outbox, revokes sessions and consumes reset links once', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare('INSERT INTO passkeys VALUES (?,?,?,?,?)')
      .run('legacy-owner', owner.session.user.id, '{}', 'Old key', 0),
  );
  assert.equal(
    (await f.request('/api/auth/forgot-password', { email: 'owner@dispatch.test' })).status,
    202,
  );
  const mail = path.join(f.root, 'data/platform/development-mail');
  const completed = () =>
    fs.existsSync(mail) ? fs.readdirSync(mail).filter((name) => name.endsWith('.json')) : [];
  await until(async () => completed().length > 0);
  const filename = path.join(mail, completed()[0]!);
  assert.equal(fs.statSync(filename).mode & 0o077, 0);
  const message = JSON.parse(fs.readFileSync(filename, 'utf8'));
  assert.match(message.html, />Reset password<\/a>/);
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(message.text)![1];
  assert.equal(
    (await f.request('/api/auth/reset-password', { token: raw, password: 'Replacement-password!' }))
      .status,
    200,
  );
  assert.equal((await owner.get('/api/session')).status, 401);
  assert.equal((await f.request('/api/auth/reset-password', { token: raw, password })).status, 400);
  const renewed = await f.client('owner@dispatch.test', 'Replacement-password!');
  assert.equal((await renewed.get('/api/platform/dsps')).status, 200);
  assert.equal(
    (await renewed.post('/api/auth/password', { currentPassword: 'wrong', password })).status,
    403,
  );
  assert.equal(
    (
      await renewed.post('/api/auth/password', {
        currentPassword: 'Replacement-password!',
        password,
      })
    ).status,
    200,
  );
  assert.equal((await renewed.get('/api/session')).status, 401);
});

test('password recovery gives existing and absent accounts the same delayed public answer', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const answers: { status: number; value: unknown; elapsed: number }[] = [];
  for (const email of ['owner@dispatch.test', 'absent@dispatch.test']) {
    const started = performance.now();
    const answer = await f.request('/api/auth/forgot-password', { email });
    answers.push({
      status: answer.status,
      value: answer.value,
      elapsed: performance.now() - started,
    });
  }
  for (const answer of answers) {
    assert.equal(answer.status, 202);
    assert.deepEqual(answer.value, { ok: true });
    assert(answer.elapsed >= 280, `recovery response returned in ${answer.elapsed}ms`);
  }
});
