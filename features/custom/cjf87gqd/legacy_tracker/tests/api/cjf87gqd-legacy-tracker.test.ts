import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../../../core/shell/tests/support/support.js';

const route = '/api/dsp/cjf87gqd-legacy-tracker';

test('Legacy Tracker answers the address its DSP set, and only at that DSP', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  assert.equal(
    (await owner.post(`/api/platform/dsps/${north.id}/custom-id`, { customId: 'cjf87gqd' })).status,
    200,
  );
  assert.equal(
    (
      await owner.post(`/api/platform/dsps/${north.id}/features`, {
        feature: 'cjf87gqd_legacy_tracker',
        enabled: true,
      })
    ).status,
    200,
  );
  await owner.select(north.id);
  const unset = await owner.get(route);
  assert.equal(unset.status, 200, unset.body);
  assert.equal(unset.value.link, null);
  const link = 'https://tracker.example.com/sheet';
  const saved = await owner.post('/api/dsp/settings/cjf87gqd_legacy_tracker.general', {
    values: { link },
    revision: 0,
  });
  assert.equal(saved.status, 200, saved.body);
  assert.equal((await owner.get(route)).value.link, link);
  const other = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(other.id);
  assert.equal((await owner.get(route)).status, 404);
});
