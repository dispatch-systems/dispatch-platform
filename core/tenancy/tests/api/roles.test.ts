import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { demo, fixture } from '../../../shell/tests/support/support.js';

type Role = { id: string; name: string; owner: boolean; permissions: string[]; members: number };

test('custom roles gate tenant APIs and never grant more than the actor holds', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  assert.equal(north.role, 'Member');
  await owner.select(north.id);
  const roles = async (): Promise<Role[]> => (await owner.get('/api/dsp/roles')).value;
  const named = async (name: string) => (await roles()).find((role) => role.name === name)!;
  assert.deepEqual(
    (await roles()).map((role) => [role.name, role.owner, role.permissions.length]),
    [
      ['Owner', true, 21],
      ['Manager', false, 4],
      ['Member', false, 2],
    ],
  );

  let view = await member.select(north.id);
  assert.deepEqual(view.permissions, ['uniforms.view', 'timecard.view']);
  assert.deepEqual(view.role, { id: (await named('Member')).id, name: 'Member', owner: false });
  assert.equal((await member.get('/api/dsp/employees')).status, 200);
  for (const url of ['/api/dsp/jobs', '/api/dsp/roles', '/api/dsp/members'])
    assert.equal((await member.get(url)).status, 403, url);

  for (const [body, status] of [
    [{ name: 'Owner', permissions: [] }, 400],
    [{ name: 'Manager', permissions: [] }, 409],
    [{ name: 'Lead', permissions: ['everything'] }, 400],
    [{ name: 'Auditor', permissions: ['audit.view'] }, 400],
  ] as const)
    assert.equal((await owner.post('/api/dsp/roles', body)).status, status);
  const created = await owner.post('/api/dsp/roles', {
    name: 'Team Lead',
    permissions: ['timecard.manage', 'members.invite', 'members.manage', 'roles.manage'],
  });
  assert.equal(created.status, 201);
  const lead: Role = created.value;
  // Managing the timecard always includes viewing it.
  assert.deepEqual(lead.permissions, [
    'timecard.view',
    'timecard.manage',
    'members.invite',
    'members.manage',
    'roles.manage',
  ]);

  const membership = (await owner.get('/api/dsp/members')).value.find(
    (row: { email: string }) => row.email === 'member@dispatch.test',
  );
  assert.equal(
    (await owner.post(`/api/dsp/members/${membership.id}`, { role: lead.id })).status,
    200,
  );
  // Role changes expire every open view of the DSP.
  assert.equal((await member.get('/api/dsp/employees')).value.error, 'dsp_view_expired');
  view = await member.select(north.id);
  assert.deepEqual(view.permissions, lead.permissions);
  assert.equal((await member.get('/api/dsp/schedules')).status, 200);
  assert.equal((await member.get('/api/dsp/connections')).status, 403);
  assert.equal((await member.post('/api/dsp/jobs', { requestId: 'denied' })).status, 403);

  await owner.select(north.id);
  const ownerRole = await named('Owner');
  const manager = await named('Manager');
  const denied = async (url: string, body: unknown) => {
    const result = await member.post(url, body);
    assert.equal(result.status, 403, url);
    assert.equal(result.value.error, 'role_exceeds_permissions');
  };
  await denied('/api/dsp/roles', { name: 'Sneaky', permissions: ['connections.manage'] });
  await denied(`/api/dsp/roles/${manager.id}`, { name: 'Manager', permissions: [] });
  await denied(`/api/dsp/roles/${manager.id}/remove`, {});
  await denied(`/api/dsp/members/${membership.id}`, { role: ownerRole.id });
  await denied(`/api/dsp/members/${membership.id}`, { role: manager.id });
  await denied('/api/dsp/members/invite', { email: 'x@dispatch.test', role: ownerRole.id });
  const viewer = await member.post('/api/dsp/roles', { name: 'Viewer', permissions: [] });
  assert.equal(viewer.status, 201);

  assert.equal(
    (await owner.post(`/api/dsp/roles/${ownerRole.id}`, { name: 'Boss', permissions: [] })).value
      .error,
    'owner_role_locked',
  );
  assert.equal(
    (await owner.post(`/api/dsp/roles/${lead.id}/remove`, {})).value.error,
    'role_in_use',
  );

  // Editing a role reaches its members as soon as they reopen the DSP.
  assert.equal(
    (await owner.post(`/api/dsp/roles/${lead.id}`, { name: 'Team Lead', permissions: [] })).status,
    200,
  );
  view = await member.select(north.id);
  assert.deepEqual(view.permissions, []);
  assert.equal(
    (await member.post('/api/dsp/presence', { tab: 'roles', state: 'active' })).status,
    200,
  );
  assert.equal((await member.get('/api/dsp/employees')).status, 403);
  assert.equal((await member.get('/api/dsp/roles')).status, 403);

  await owner.select(north.id);
  assert.equal(
    (await owner.post(`/api/dsp/members/${membership.id}`, { role: viewer.value.id })).status,
    200,
  );
  await owner.select(north.id);
  assert.equal((await owner.post(`/api/dsp/roles/${lead.id}/remove`, {})).status, 200);
  assert.equal((await named('Viewer')).members, 1);
});

test('role edits need no fresh verification and invitations follow their sender authority', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(summit.id);
  const summitMember = (await owner.get('/api/dsp/roles')).value.find(
    (role: Role) => role.name === 'Member',
  );
  const roles = async (): Promise<(Role & { invitations: number })[]> => {
    await owner.select(north.id);
    return (await owner.get('/api/dsp/roles')).value;
  };
  const write = async (url: string, body: unknown) => {
    await owner.select(north.id);
    return owner.post(url, body);
  };
  const create = async (name: string, permissions: string[]): Promise<Role> =>
    (await write('/api/dsp/roles', { name, permissions })).value;
  const crew = await create('Crew', ['uniforms.view']);
  const temp = await create('Temp', ['uniforms.view']);
  const lead = await create('Lead', ['uniforms.view', 'members.invite']);
  await owner.select(north.id);
  const sender = (await owner.get('/api/dsp/members')).value.find(
    (m: { email: string }) => m.email === 'member@dispatch.test',
  );
  assert.equal((await write(`/api/dsp/members/${sender.id}`, { role: lead.id })).status, 200);

  // The sender also works for Summit, so leaving Northline keeps their account.
  const tokens = ['c'.repeat(43), 'd'.repeat(43), 'e'.repeat(43)];
  f.database('data/platform/accounts.sqlite', (db) => {
    db.prepare(
      "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES ('mem_summit',?,?,'member',?)",
    ).run(sender.userId, summit.id, summitMember.id);
    const invite = db.prepare(
      "INSERT INTO invitations(hash,dsp_id,email,role,role_id,expires_at,created_by) VALUES (?,?,?,'member',?,?,?)",
    );
    for (const [token, email, role, by] of [
      [tokens[0], 'from-owner@dispatch.test', crew.id, owner.session.user.id],
      [tokens[1], 'from-sender@dispatch.test', crew.id, sender.userId],
      [tokens[2], 'temp@dispatch.test', temp.id, owner.session.user.id],
    ])
      invite.run(
        createHash('sha256').update(token!).digest('hex'),
        north.id,
        email,
        role,
        Date.now() + 60000,
        by,
      );
    db.prepare('UPDATE sessions SET created_at=0 WHERE user_id=?').run(owner.session.user.id);
  });

  // Long after signing in, role permissions still switch on and off without a password.
  for (const permissions of [[], ['uniforms.view', 'timecard.view']])
    assert.equal(
      (await write(`/api/dsp/roles/${crew.id}`, { name: 'Crew', permissions })).status,
      200,
    );
  const scratch = await create('Scratch', []);
  assert.equal((await write(`/api/dsp/roles/${scratch.id}/remove`, {})).status, 200);
  const invite = await write('/api/dsp/members/invite', {
    email: 'later@dispatch.test',
    role: crew.id,
  });
  assert.equal(invite.status, 403, invite.body);
  assert.ok(['sign_in_again', 'reauthentication_required'].includes(invite.value.error));
  assert.equal(
    (await owner.post('/api/auth/security/reauthenticate', { password: demo.password })).status,
    200,
  );

  // An invitation never outlives its sender's authority, even when their account stays because
  // they belong to another DSP. Platform-owner invitations remain usable through role edits.
  const memberRole = (await roles()).find((role) => role.name === 'Member')!;
  assert.equal((await write(`/api/dsp/members/${sender.id}`, { role: memberRole.id })).status, 200);
  const burst = await Promise.all(
    Array.from({ length: 32 }, (_, index) => f.request(`/api/invitations/${tokens[index % 2]}`)),
  );
  assert.deepEqual(
    burst.map((result) => result.status),
    Array.from({ length: 32 }, (_, index) => (index % 2 === 0 ? 200 : 404)),
  );
  assert.equal((await write(`/api/dsp/members/${sender.id}`, { role: null })).status, 200);
  assert.equal((await f.request(`/api/invitations/${tokens[0]}`)).status, 200);
  assert.equal((await f.request(`/api/invitations/${tokens[1]}`)).status, 404);
  assert.equal((await f.request(`/api/invitations/${tokens[2]}`)).status, 200);
  const denied = await f.request(`/api/invitations/${tokens[1]}/accept`, {
    firstName: 'Casey',
    lastName: 'Rivers',
    password: demo.password,
  });
  assert.equal(denied.status, 404, denied.body);
  const accepted = await f.request(`/api/invitations/${tokens[0]}/accept`, {
    firstName: 'Alex',
    lastName: 'Rivera',
    password: demo.password,
  });
  assert.equal(accepted.status, 200, accepted.body);
  await owner.select(north.id);
  const joined = (await owner.get('/api/dsp/members')).value.find(
    (m: { email: string }) => m.email === 'from-owner@dispatch.test',
  );
  assert.equal(joined.roleId, crew.id);

  // Deleting a role with pending invitations is allowed and cancels them.
  assert.equal((await roles()).find((role) => role.id === temp.id)!.invitations, 1);
  assert.equal((await write(`/api/dsp/roles/${temp.id}/remove`, {})).status, 200);
  assert.equal((await f.request(`/api/invitations/${tokens[2]}`)).status, 404);
  assert.equal((await f.request(`/api/invitations/${tokens[0]}`)).status, 200);
  assert.equal((await write(`/api/dsp/roles/${crew.id}/remove`, {})).value.error, 'role_in_use');
});

test('disabled feature grants still count against the durable delegation ceiling', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  await owner.select(north.id);
  const privileged = (
    await owner.post('/api/dsp/roles', {
      name: 'Uniform Manager',
      permissions: ['uniforms.manage'],
    })
  ).value;
  const delegated = (
    await owner.post('/api/dsp/roles', {
      name: 'Role Manager',
      permissions: ['members.invite', 'members.manage', 'roles.manage'],
    })
  ).value;
  const membership = (await owner.get('/api/dsp/members')).value.find(
    (row: { email: string }) => row.email === 'member@dispatch.test',
  );
  assert.equal(
    (await owner.post(`/api/dsp/members/${membership.id}`, { role: delegated.id })).status,
    200,
  );
  assert.equal(
    (
      await owner.post(`/api/platform/dsps/${north.id}/features`, {
        feature: 'uniforms',
        enabled: false,
      })
    ).status,
    200,
  );
  await member.select(north.id);

  for (const [url, body] of [
    [`/api/dsp/roles/${privileged.id}`, { name: 'Hidden Uniform Manager', permissions: [] }],
    [`/api/dsp/roles/${privileged.id}/remove`, {}],
    [`/api/dsp/members/${membership.id}`, { role: privileged.id }],
    ['/api/dsp/members/invite', { email: 'hidden@dispatch.test', role: privileged.id }],
  ] as const) {
    const result = await member.post(url, body);
    assert.equal(result.status, 403, `${url}: ${result.body}`);
    assert.equal(result.value.error, 'role_exceeds_permissions');
  }

  // Owners may still edit roles while a feature is off, and hidden grants remain stored.
  await owner.select(north.id);
  assert.equal(
    (
      await owner.post(`/api/dsp/roles/${privileged.id}`, {
        name: 'Hidden Uniform Manager',
        permissions: [],
      })
    ).status,
    200,
  );
  assert.equal(
    (
      await owner.post(`/api/platform/dsps/${north.id}/features`, {
        feature: 'uniforms',
        enabled: true,
      })
    ).status,
    200,
  );
  await owner.select(north.id);
  const restored = (await owner.get('/api/dsp/roles')).value.find(
    (role: Role) => role.id === privileged.id,
  );
  assert.deepEqual(restored.permissions, ['uniforms.view', 'uniforms.manage']);
});

test('memberships written without a role id resolve through the legacy role after restart', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const north = (await f.client('member@dispatch.test')).session.dsps[0];
  await f.stop();
  f.database('data/platform/accounts.sqlite', (db) => {
    db.exec('UPDATE memberships SET role_id=NULL');
    db.exec("DELETE FROM roles WHERE name='Member'");
  });
  await f.start();
  const member = await f.client('member@dispatch.test');
  assert.deepEqual((await member.select(north.id)).permissions, ['uniforms.view', 'timecard.view']);
  const rows = f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare(
        'SELECT m.role,r.name FROM memberships m JOIN roles r ON r.id=m.role_id AND r.dsp_id=m.dsp_id',
      )
      .all(),
  );
  assert.deepEqual(
    rows.map((row) => ({ ...row })),
    [{ role: 'member', name: 'Member' }],
  );
});
