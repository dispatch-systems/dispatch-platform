import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';
import { capturedMail } from '../../../../core/shell/tests/support/mail-support.js';

test('removal revokes legacy inbound and authored invitations without transferring attribution', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const dsp = member.session.dsps[0].id;
  await owner.select(dsp);
  const members = (await owner.get('/api/dsp/members')).value;
  const person = members.find((m: { email: string }) => m.email === 'member@dispatch.test');
  const tokens = ['a'.repeat(43), 'b'.repeat(43)];
  f.database('data/platform/accounts.sqlite', (db) => {
    const insert = db.prepare(
      "INSERT INTO invitations(hash,dsp_id,email,role,role_id,expires_at,created_by) VALUES (?,?,?,'member',?,?,?)",
    );
    insert.run(
      createHash('sha256').update(tokens[0]!).digest('hex'),
      dsp,
      person.email,
      person.roleId,
      Date.now() + 60000,
      owner.session.user.id,
    );
    insert.run(
      createHash('sha256').update(tokens[1]!).digest('hex'),
      dsp,
      'colleague@dispatch.test',
      person.roleId,
      Date.now() + 60000,
      person.userId,
    );
  });
  assert.equal(
    (await owner.post('/api/dsp/members/invite', { email: person.email, role: person.roleId }))
      .status,
    409,
  );
  assert.equal((await owner.post(`/api/dsp/members/${person.id}`, { role: null })).status, 200);
  assert.equal((await member.get('/api/session')).status, 401);
  for (const token of tokens) {
    assert.equal((await f.request(`/api/invitations/${token}`)).status, 404);
    assert.equal(
      (
        await f.request(`/api/invitations/${token}/accept`, {
          firstName: 'Removed',
          lastName: 'Member',
          password: demo.password,
        })
      ).status,
      404,
    );
  }
});

test('invitation password checks share login limits; duplicates cool down and replace prior grants', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const dsp = owner.session.dsps.find(
    (d: { id: string }) => !member.session.dsps.some((m: { id: string }) => m.id === d.id),
  );
  await owner.select(dsp.id);
  const roles = (await owner.get('/api/dsp/roles')).value;
  const body = {
    email: 'member@dispatch.test',
    role: roles.find((r: { name: string }) => r.name === 'Member').id,
  };
  assert.equal((await owner.post('/api/dsp/members/invite', body)).status, 200);
  assert.equal((await owner.post('/api/dsp/members/invite', body)).status, 429);
  const token = /token=([A-Za-z0-9_-]{43})/.exec((await capturedMail(f.root, body.email)).text)![1];
  f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare(
        "INSERT OR REPLACE INTO throttle(key,count,reset_at,namespace) VALUES (?,?,?,'login')",
      )
      .run(
        createHash('sha256').update(`login:email:${body.email}`).digest('hex'),
        10,
        Date.now() + 900000,
      ),
  );
  // An invitation to a DSP without a short code yet opens at the invite page.
  const at = f.at(dsp.code ?? 'invite');
  assert.equal(
    (
      await f.request(
        `/api/invitations/${token}/accept`,
        { firstName: 'Existing', lastName: 'Member', password: demo.password },
        at,
      )
    ).status,
    429,
  );
  f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare('DELETE FROM throttle WHERE key=?')
      .run(createHash('sha256').update(`mail:cooldown:${body.email}`).digest('hex')),
  );
  assert.equal((await owner.post('/api/dsp/members/invite', body)).status, 200);
  assert.equal((await f.request(`/api/invitations/${token}`, undefined, at)).status, 404);
  const count = f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare('SELECT count(*) n FROM invitations WHERE email=? AND used_at IS NULL')
      .get(body.email),
  );
  assert.equal(count?.n, 1);
});
