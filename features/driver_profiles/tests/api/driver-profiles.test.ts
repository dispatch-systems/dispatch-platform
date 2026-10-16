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
  const status = await owner.read('/api/dsp/driver-profiles/status');
  assert.ok(status.collectedAt);
  assert.deepEqual(
    [status.active, status.inactive, status.photos, status.photosWaiting],
    [7, 1, 8, 0],
  );
  // The photos are files in the DSP's cache, which backups leave out.
  const photos = path.join(f.root, 'dsps', north.id, 'cache', 'driver_profiles', 'photos');
  assert.equal(readdirSync(photos).length, 8);
  // No default role holds driver_profiles.collect, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/driver-profiles/status')).status, 403);
  assert.equal(
    (await member.post('/api/dsp/driver-profiles/collect', { requestId: 'member' })).status,
    403,
  );
});
