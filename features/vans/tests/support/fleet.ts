import assert from 'node:assert/strict';
import { fixture } from '../../../../core/shell/tests/support/support.js';

/** Northline Logistics with its abbreviation and a Cortex connection, ready to collect vans:
 * its owner and the DSP. */
export async function connectFleet(app: Awaited<ReturnType<typeof fixture>>) {
  const owner = await app.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  const profile = { name: dsp.name, abbreviation: 'NLL', stationCode: 'TST1' };
  assert.equal(
    (await owner.post('/api/dsp/profile', { ...profile, timezone: dsp.timezone })).status,
    200,
  );
  await owner.select(dsp.id);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(connected.status, 200, connected.body);
  return { owner, dsp: dsp as { id: string; name: string } };
}
