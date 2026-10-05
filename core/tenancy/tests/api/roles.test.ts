import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../shell/tests/support/support.js';

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
