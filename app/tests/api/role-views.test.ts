import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';

test('a platform owner looks through any DSP role with exactly that role’s access', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  const full = await owner.select(north.id);
  assert.equal(full.role.owner, true);
  const custom = await owner.post('/api/dsp/roles', {
    name: 'Connections admin',
    permissions: ['connections.manage'],
  });
  assert.equal(custom.status, 201, JSON.stringify(custom.value));
  const opened = await owner.select(north.id);
  assert.deepEqual(
    opened.roles.map((role: { name: string }) => role.name),
    ['Owner', 'Manager', 'Member', 'Connections admin'],
  );
  const preview = async (roleId: string) => {
    const view = await owner.post('/api/session/dsp', { dspId: north.id, roleId });
    assert.equal(view.status, 200, JSON.stringify(view.value));
    owner.headers['x-dispatch-view'] = view.value.token;
    return view.value;
  };
  const connectionsAdmin = await preview(custom.value.id);
  assert.deepEqual(connectionsAdmin.role, {
    id: custom.value.id,
    name: 'Connections admin',
    owner: false,
  });
  assert.deepEqual(connectionsAdmin.permissions, ['connections.manage']);
  assert.equal(connectionsAdmin.roles.length, 4);
  assert.equal((await owner.get('/api/dsp/employees')).status, 403);
  assert.equal((await owner.get('/api/dsp/roles')).status, 403);
  assert.equal((await owner.get('/api/dsp/connections')).status, 200);

  // The previewed role is part of the signed view and cannot be traded up.
  const manager = opened.roles.find((role: { name: string }) => role.name === 'Manager');
  const token = owner.headers['x-dispatch-view']!;
  owner.headers['x-dispatch-view'] = token.replace(custom.value.id, manager.id);
  assert.equal((await owner.get('/api/dsp/employees')).status, 409);
  owner.headers['x-dispatch-view'] = [token.split('.')[0], token.split('.')[2]].join('.');
  assert.equal((await owner.get('/api/dsp/employees')).status, 409);

  await preview(manager.id);
  assert.equal((await owner.get('/api/dsp/employees')).status, 200);
  assert.equal((await owner.get('/api/dsp/connections')).status, 403);

  // Deleting the previewed role expires the view, and it cannot be reopened.
  await owner.select(north.id);
  const stale = (await preview(custom.value.id)).token;
  await owner.select(north.id);
  assert.equal((await owner.post(`/api/dsp/roles/${custom.value.id}/remove`)).status, 200);
  owner.headers['x-dispatch-view'] = stale;
  assert.equal((await owner.get('/api/dsp/connections')).status, 409);
  assert.equal(
    (await owner.post('/api/session/dsp', { dspId: north.id, roleId: custom.value.id })).status,
    409,
  );

  const member = await f.client('member@dispatch.test');
  assert.equal(
    (await member.post('/api/session/dsp', { dspId: north.id, roleId: manager.id })).status,
    403,
  );
  assert.equal((await member.select(north.id)).roles, undefined);
});
