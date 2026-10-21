import test from 'node:test';
import assert from 'node:assert/strict';
import { collectNow } from '../../../../core/collection/tests/support/collect.js';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';
import { connectFleet } from '../support/fleet.js';

test('the fleet collects, for those who may collect it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const { owner, dsp } = await connectFleet(f);
  await collectNow(owner, 'vans', 'fleet');
  // The vans it lists that the DSP runs, apart from those it lists as inactive.
  const counted = f.database(`dsps/${dsp.id}/data/vans/vans.sqlite`, (db) =>
    [
      "SELECT count(*) AS n FROM vans WHERE listed=1 AND status<>'INACTIVE'",
      "SELECT count(*) AS n FROM vans WHERE listed=1 AND status='INACTIVE'",
    ].map((sql) => Number((db.prepare(sql).get() as { n: number }).n)),
  );
  assert.deepEqual(counted, [5, 1]);
  // No default role holds vans.collect, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(dsp.id);
  const now = { collects: 'vans', requestId: 'member' };
  assert.equal((await member.post('/api/dsp/collectors/vans/collect', now)).status, 403);
});
