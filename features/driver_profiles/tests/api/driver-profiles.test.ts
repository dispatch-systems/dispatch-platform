import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import path from 'node:path';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';
import { collectRoster } from '../support/roster.js';

test('the roster collects with its photos, for those who may collect it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = await collectRoster(owner);
  // Those it lists, active and not, their photos, and none left for the next collection.
  const counted = f.database(`dsps/${north.id}/data/driver_profiles/driver_profiles.sqlite`, (db) =>
    [
      "SELECT count(*) AS n FROM associates WHERE listed=1 AND status='ACTIVE'",
      "SELECT count(*) AS n FROM associates WHERE listed=1 AND status<>'ACTIVE'",
      'SELECT count(*) AS n FROM photos',
      'SELECT photos_waiting AS n FROM roster_runs ORDER BY collected_at DESC LIMIT 1',
    ].map((sql) => Number((db.prepare(sql).get() as { n: number }).n)),
  );
  assert.deepEqual(counted, [7, 1, 8, 0]);
  // The photos are files in the DSP's cache, which backups leave out.
  const photos = path.join(f.root, 'dsps', north.id, 'cache', 'driver_profiles', 'photos');
  assert.equal(readdirSync(photos).length, 8);
  // No default role holds driver_profiles.collect, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(north.id);
  const now = { collects: 'associates', requestId: 'member' };
  assert.equal((await member.post('/api/dsp/collectors/associates/collect', now)).status, 403);
});
