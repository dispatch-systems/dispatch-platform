import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../core/shell/tests/support/support.js';

test('members report presence to their team; platform owners never appear', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  // A platform owner can also hold a real membership in a DSP they look after.
  f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare(
        "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) SELECT 'mem_platform_owner',?,dsp_id,'owner',id FROM roles WHERE dsp_id=? AND system=1",
      )
      .run(owner.session.user.id, north.id),
  );
  await owner.select(north.id);
  await member.select(north.id);
  const statuses = async () =>
    Object.fromEntries(
      (await owner.get('/api/dsp/members')).value.map((row: { email: string; status: string }) => [
        row.email,
        row.status,
      ]),
    );
  const everyoneOffline = { 'owner@dispatch.test': 'offline', 'member@dispatch.test': 'offline' };
  assert.deepEqual(await statuses(), everyoneOffline);

  const beat = (tab: string, state: string) => member.post('/api/dsp/presence', { tab, state });
  assert.equal((await beat('one', 'idle')).status, 200);
  assert.equal((await statuses())['member@dispatch.test'], 'idle');
  // The most active of a member's open dashboards wins.
  assert.equal((await beat('two', 'active')).status, 200);
  assert.equal((await statuses())['member@dispatch.test'], 'active');
  await beat('two', 'gone');
  assert.equal((await statuses())['member@dispatch.test'], 'idle');
  await beat('one', 'gone');

  assert.equal(
    (await owner.post('/api/dsp/presence', { tab: 'one', state: 'active' })).status,
    200,
  );
  assert.deepEqual(await statuses(), everyoneOffline);

  assert.equal((await beat('one', 'busy')).status, 400);
  const { 'x-dispatch-view': _view, ...unscoped } = member.headers;
  assert.notEqual(
    (await f.request('/api/dsp/presence', { tab: 'one', state: 'active' }, unscoped)).status,
    200,
  );
});
