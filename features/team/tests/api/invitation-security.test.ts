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
  const { id: dsp, code } = member.session.dsps[0];
  await owner.select(dsp);
  const members = (await owner.get('/api/dsp/members')).value;
  const person = members.find((m: { email: string }) => m.email === 'member@dispatch.test');
  // The member may invite to their own role, so the invitation they sent is open until they go.
  const role = (await owner.get('/api/dsp/roles')).value.find(
    (r: { id: string }) => r.id === person.roleId,
  );
  assert.equal(
    (
      await owner.post(`/api/dsp/roles/${role.id}`, {
        name: role.name,
        permissions: [...role.permissions, 'members.invite'],
      })
    ).status,
    200,
  );
  // A change to the DSP's roles or members expires the owner's view of it.
  await owner.select(dsp);
  const tokens = ['a'.repeat(43), 'b'.repeat(43)];
  const hashes = tokens.map((token) => createHash('sha256').update(token).digest('hex'));
  f.database('data/platform/accounts.sqlite', (db) => {
    const route = db.prepare('INSERT INTO invitation_routes(hash,dsp_id) VALUES (?,?)');
    for (const hash of hashes) route.run(hash, dsp);
  });
  f.people(dsp, (db) => {
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
  // Both are open at the DSP's address, and listed, until the removal.
  const at = f.at(code);
  const emails = ['colleague@dispatch.test', person.email];
  const open = async () =>
    (await owner.read('/api/dsp/invitations'))
      .map((i: { email: string }) => i.email)
      .filter((email: string) => emails.includes(email))
      .sort();
  for (const token of tokens)
    assert.equal((await f.request(`/api/invitations/${token}`, undefined, at)).status, 200);
  assert.deepEqual(await open(), emails);
  assert.equal((await owner.post(`/api/dsp/members/${person.id}`, { role: null })).status, 200);
  assert.equal((await member.get('/api/session')).status, 401);
  await owner.select(dsp);
  // Revoked, not only refused: the one they sent is gone, not left without its sender.
  assert.deepEqual(await open(), []);
  for (const token of tokens) {
    assert.equal((await f.request(`/api/invitations/${token}`, undefined, at)).status, 404);
    assert.equal(
      (
        await f.request(
          `/api/invitations/${token}/accept`,
          { firstName: 'Removed', lastName: 'Member', password: demo.password },
          at,
        )
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
        // An account's attempts are counted in its DSP's directory.
        createHash('sha256').update(`login:email:${dsp.id}:${body.email}`).digest('hex'),
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
  const count = f.people(dsp.id, (db) =>
    db
      .prepare('SELECT count(*) n FROM invitations WHERE email=? AND used_at IS NULL')
      .get(body.email),
  );
  assert.equal(count?.n, 1);
});
