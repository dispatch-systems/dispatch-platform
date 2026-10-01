import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../support/support.js';
import type { AuditEvent, DriverDetails, DriverMatch } from '../../shared/contracts/index.js';

const code = /^[2-9A-HJKMNP-TV-Z]{6}$/;

test('Driver Match gives everyone a code and answers only to its own permission', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  await member.select(north.id);
  // A member's role holds no Driver Match permission.
  assert.equal((await member.get('/api/dsp/driver-match')).status, 403);
  await owner.select(north.id);
  // The scheduler gives the demo's people their codes shortly after the server starts.
  let result!: DriverMatch;
  await until(async () => {
    result = await owner.read('/api/dsp/driver-match');
    return Boolean(result.checkedAt) && result.drivers.length > 1;
  });
  assert.ok(result.drivers.every((driver) => code.test(driver.code)));
  assert.equal(result.counts.all, result.drivers.length);
  const [first, second] = result.drivers;
  const details: DriverDetails = (await owner.get(`/api/dsp/driver-match/drivers/${first!.code}`))
    .value;
  assert.equal(details.driver.code, first!.code);
  assert.equal(details.days.length, 14);
  assert.equal((await owner.get('/api/dsp/driver-match/drivers/NOPE00')).status, 404);

  // Two people kept apart: the decision is theirs to see in each one's history.
  const apart = await owner.post('/api/dsp/driver-match/apart', {
    code: first!.code,
    other: second!.code,
  });
  assert.equal(apart.status, 200, apart.body);
  const history: DriverDetails = (await owner.get(`/api/dsp/driver-match/drivers/${second!.code}`))
    .value;
  assert.ok(history.history.some((e) => e.kind === 'apart' && e.code === first!.code));
  // A write needs the session's CSRF token, and a decision on a page out of date is refused.
  const headers = { ...owner.headers };
  delete headers['x-csrf-token'];
  const merge = { code: second!.code, into: first!.code };
  assert.equal((await f.request('/api/dsp/driver-match/merge', merge, headers)).status, 403);
  assert.equal((await owner.post('/api/dsp/driver-match/merge', merge)).status, 200);
  // The activity log names the person the decision made, as the tab shows them now.
  const merged: DriverMatch = (await owner.get('/api/dsp/driver-match')).value;
  const log = await owner.get('/api/platform/audit');
  assert.equal(log.status, 200, log.body);
  const event = log.value.events.find((e: AuditEvent) => e.action === 'driver_match.merged');
  assert.equal(event.target, merged.drivers.find((d) => d.code === first!.code)!.name);
  assert.deepEqual(event.ref, { kind: 'driver', id: first!.code });
  const stale = await owner.post('/api/dsp/driver-match/merge', merge);
  assert.deepEqual([stale.status, stale.value.error], [409, 'driver_changed']);
  for (const body of [
    { code: first!.code, into: first!.code },
    { code: 'short', into: second!.code },
    { code: first!.code, into: second!.code, extra: true },
  ])
    assert.equal((await owner.post('/api/dsp/driver-match/merge', body)).status, 400);
  assert.equal(
    (await owner.post('/api/dsp/driver-match/split', { code: first!.code, source: 'x', id: 'y' }))
      .status,
    400,
  );

  // Switched off, the tab's routes are gone; the codes stay for when it comes back.
  const kept: DriverMatch = (await owner.get('/api/dsp/driver-match')).value;
  const url = `/api/platform/dsps/${north.id}/features`;
  await owner.post(url, { feature: 'driver_match', enabled: false });
  await owner.select(north.id);
  assert.equal((await owner.get('/api/dsp/driver-match')).status, 403);
  await owner.post(url, { feature: 'driver_match', enabled: true });
  await owner.select(north.id);
  const again: DriverMatch = (await owner.get('/api/dsp/driver-match')).value;
  assert.deepEqual(again.drivers.map((d) => d.code).sort(), kept.drivers.map((d) => d.code).sort());
});
