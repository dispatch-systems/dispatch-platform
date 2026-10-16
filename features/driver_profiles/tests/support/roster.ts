import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

type App = Awaited<ReturnType<typeof fixture>>;
type Client = Awaited<ReturnType<App['client']>>;

/**
 * Collects Northline's roster from the Cortex fixture as `owner`, who connects Cortex and gives
 * the DSP its abbreviation first, and waits until it is in. Answers the DSP.
 */
export async function collectRoster(owner: Client) {
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const profile = { name: north.name, abbreviation: 'NLL', stationCode: 'TST1' };
  assert.equal(
    (await owner.post('/api/dsp/profile', { ...profile, timezone: north.timezone })).status,
    200,
  );
  await owner.select(north.id);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(connected.status, 200, connected.body);
  const job = await owner.post('/api/dsp/driver-profiles/collect', { requestId: 'roster' });
  assert.equal(job.status, 202, job.body);
  await until(async () => {
    const status = await owner.read('/api/dsp/driver-profiles/status');
    const current = status.jobs.find((item: { id: string }) => item.id === job.value.id);
    assert.notEqual(current?.status, 'failed', JSON.stringify(current));
    return current?.status === 'succeeded';
  });
  return north as { id: string; name: string };
}
