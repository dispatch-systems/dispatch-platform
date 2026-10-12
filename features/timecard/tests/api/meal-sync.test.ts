import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

test('Meal Breaks sync authorizes both collectors and publishes their selected date', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  const request = { requestId: 'both-sources', date: '2026-01-11' };
  assert.equal((await member.post('/api/dsp/jobs/meal-breaks', request)).status, 403);
  assert.equal(
    (await owner.post('/api/dsp/jobs/meal-breaks', request)).value.error,
    'meal_sync_flex_required',
  );
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(
    (await owner.post('/api/dsp/jobs/meal-breaks', request)).value.error,
    'meal_sync_scope_required',
  );
  const profile = await owner.post('/api/dsp/profile', {
    name: dsp.name,
    abbreviation: 'NLL',
    stationCode: 'DEMO1',
    timezone: 'America/Los_Angeles',
  });
  assert.equal(profile.status, 200, profile.body);
  await owner.select(dsp.id);
  await member.select(dsp.id);
  assert.equal(
    (await owner.get('/api/dsp/jobs/meal-breaks?date=2026-01-11')).value.scopeAvailable,
    true,
  );
  assert.deepEqual((await owner.get('/api/dsp/cortex/meal-breaks?date=2026-01-11')).value, []);
  const queued = await owner.post('/api/dsp/jobs/meal-breaks', request);
  assert.equal(queued.status, 202, queued.body);
  assert.equal(queued.value.jobs.length, 2);
  assert.deepEqual(
    (await owner.post('/api/dsp/jobs/meal-breaks', request)).value.jobs.map((j: any) => j.id),
    queued.value.jobs.map((j: any) => j.id),
  );
  await until(async () => {
    const jobs = (await owner.read('/api/dsp/jobs')).filter((j: any) =>
      queued.value.jobs.some((q: any) => q.id === j.id),
    );
    assert(
      jobs.every((j: any) => j.status !== 'failed'),
      JSON.stringify(jobs),
    );
    return jobs.every((j: any) => j.status === 'succeeded');
  });
  const status = await owner.get('/api/dsp/jobs/meal-breaks?date=2026-01-11');
  for (const source of ['paycom', 'flex']) {
    assert.equal(status.value[source].job.status, 'succeeded');
    assert(status.value[source].collectedAt);
  }
  const comparison = (await owner.get('/api/dsp/paycom/meal-breaks?date=2026-01-11')).value;
  assert(comparison.paycomCollectedAt);
  assert.equal(comparison.cortexPublications.length, 1);
  // Publication must not change the meaning of a retried first-use request.
  assert.deepEqual(
    (await owner.post('/api/dsp/jobs/meal-breaks', request)).value.jobs.map((j: any) => j.id),
    queued.value.jobs.map((j: any) => j.id),
  );
  assert.equal((await member.get('/api/dsp/jobs/meal-breaks?date=2026-01-11')).status, 200);
  const other = owner.session.dsps.find((d: any) => d.permanent);
  await owner.select(other.id);
  const isolated = (await owner.get('/api/dsp/jobs/meal-breaks?date=2026-01-11')).value;
  assert.equal(isolated.scopeAvailable, false);
  assert.equal(isolated.flex.collectedAt, null);
  assert.equal(isolated.flex.job, null);
});

test('simultaneous manual requests from separate sessions reserve one DSP sync', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const otherSession = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Summit Delivery');
  await owner.select(dsp.id);
  await otherSession.select(dsp.id);
  await owner.post('/api/dsp/connections/paycom', {
    clientCode: 'TEST',
    username: 'fixture-user',
    password: 'require-verification',
    securityAnswers: ['one', 'two', 'three', 'four', 'five'],
  });
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture-user',
    password: 'fixture-password',
  });
  await owner.post('/api/dsp/profile', {
    name: dsp.name,
    abbreviation: 'SUM',
    stationCode: 'DEMO1',
    timezone: 'America/Los_Angeles',
  });
  await owner.select(dsp.id);
  await otherSession.select(dsp.id);
  const requests = [
    { requestId: 'first-session', date: '2026-01-11' },
    { requestId: 'second-session', date: '2026-01-12' },
  ];
  const responses = await Promise.all([
    owner.post('/api/dsp/jobs/meal-breaks', requests[0]),
    otherSession.post('/api/dsp/jobs/meal-breaks', requests[1]),
  ]);
  assert.deepEqual(responses.map((r) => r.status).sort(), [202, 409]);
  assert.equal(responses.find((r) => r.status === 409)!.value.error, 'sync_in_progress');
  const winner = responses.findIndex((r) => r.status === 202);
  const accepted = responses[winner]!.value;
  const replay = await otherSession.post('/api/dsp/jobs/meal-breaks', requests[winner]);
  assert.equal(replay.status, 202);
  assert.deepEqual(
    replay.value.jobs.map((j: any) => j.id),
    accepted.jobs.map((j: any) => j.id),
  );
  assert.equal((await owner.get('/api/dsp/jobs')).value.length, 2);
  const viewed = (await otherSession.get('/api/dsp/jobs/meal-breaks?date=2026-01-13')).value;
  assert.equal(viewed.date, '2026-01-13');
  assert.equal(viewed.flex.jobDate, requests[winner]!.date);
  assert.equal(viewed.flex.active, true);
  for (const request of [
    { requestId: 'employees' },
    { requestId: 'timecard', date: '2026-01-13' },
  ]) {
    const blocked = await otherSession.post('/api/dsp/jobs', request);
    assert.equal(blocked.status, 409);
    assert.equal(blocked.value.error, 'sync_in_progress');
  }
  const blocked = await owner.post('/api/dsp/cortex/meal-breaks/collect', {
    requestId: 'flex-only',
    date: '2026-01-13',
    station: 'DEMO1',
    serviceAreaId: 'area-demo',
    provider: 'provider-demo',
  });
  assert.equal(blocked.status, 409);
  assert.equal(blocked.value.error, 'sync_in_progress');
  // Another DSP retains its own collector access.
  const other = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await otherSession.select(other.id);
  const independent = await otherSession.post('/api/dsp/jobs', { requestId: 'independent' });
  assert.equal(independent.status, 202);
  // Cancel the queued Flex job first so it cannot start as Paycom stops.
  for (const job of [...accepted.jobs].reverse())
    assert.equal((await owner.post(`/api/dsp/jobs/${job.id}/cancel`)).status, 200);
  const restarted = await owner.post('/api/dsp/jobs/meal-breaks', {
    requestId: 'after-cancellation',
    date: '2026-01-13',
  });
  assert.equal(restarted.status, 202);
  // Leave nothing running: the server waits for a started job, and this test ends by
  // starting two. A job that already finished answers 409, which is equally quiet.
  for (const [client, ids] of [
    [owner, restarted.value.jobs.map((job: any) => job.id)],
    [otherSession, [independent.value.id]],
  ] as const)
    for (const id of [...ids].reverse())
      assert.ok([200, 409].includes((await client.post(`/api/dsp/jobs/${id}/cancel`)).status));
});

test('employee Sync Now collects every employee and both sources for the displayed historical period', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  const latest = (await owner.get('/api/dsp/employees/E001')).value;
  const period = latest.previousPeriod;
  const request = { requestId: 'historical-employees', ...period };
  const statusUrl = `/api/dsp/jobs/employees?from=${period.from}&to=${period.to}`;
  assert.equal((await member.post('/api/dsp/jobs/employees', request)).status, 403);
  assert.equal(
    (await owner.post('/api/dsp/jobs/employees', request)).value.error,
    'meal_sync_flex_required',
  );
  assert.equal((await owner.get('/api/dsp/jobs')).value.length, 0);
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  await owner.post('/api/dsp/profile', {
    name: dsp.name,
    abbreviation: 'NLL',
    stationCode: 'DEMO1',
    timezone: 'America/Los_Angeles',
  });
  await owner.select(dsp.id);
  await member.select(dsp.id);
  assert.equal((await member.get(statusUrl)).status, 200);
  const queued = await owner.post('/api/dsp/jobs/employees', request);
  assert.equal(queued.status, 202, queued.body);
  assert.deepEqual(queued.value.period, period);
  assert.equal(queued.value.through, period.to);
  assert.equal(queued.value.paycom.total, 1);
  assert.equal(queued.value.cortex.total, 14);
  const original = [...queued.value.paycom.jobs, ...queued.value.cortex.jobs].map(
    (job: any) => job.id,
  );
  await until(async () => {
    const status = (await owner.get(statusUrl)).value;
    assert.equal(status.paycom.failed, 0, JSON.stringify(status));
    assert.equal(status.cortex.failed, 0, JSON.stringify(status));
    return status.complete;
  });
  const completed = (await owner.get(statusUrl)).value;
  assert.equal(completed.paycom.completed, 1);
  assert.equal(completed.cortex.completed, 14);
  assert.equal(completed.cortex.progress, 100);
  assert.equal(completed.active, false);
  // The batch does not depend on the employee selected when clicked, nor replace
  // the directory's latest roster with the historical publication.
  const other = (await owner.get(`/api/dsp/employees/E002?from=${period.from}&to=${period.to}`))
    .value;
  assert(other.collectedAt);
  assert.equal(other.timecards.length, 14);
  assert.equal((await owner.get('/api/dsp/employees/E001')).value.period.to, latest.period.to);
  const meals = (
    await owner.get(
      `/api/dsp/paycom/meal-breaks/period?source=paycom:E002&from=${period.from}&to=${period.to}`,
    )
  ).value;
  assert.equal(meals.days.length, 14);
  assert(
    meals.days.every((day: any) => day.paycomCollectedAt && day.cortexPublications.length === 1),
  );
  const replay = await owner.post('/api/dsp/jobs/employees', request);
  assert.deepEqual(
    [...replay.value.paycom.jobs, ...replay.value.cortex.jobs].map((job: any) => job.id),
    original,
  );
  const conflict = await owner.post('/api/dsp/jobs/employees', { ...request, ...latest.period });
  assert.equal(conflict.status, 409);
  assert.equal(conflict.value.error, 'idempotency_conflict');
  const otherDsp = owner.session.dsps.find((d: any) => d.permanent);
  await owner.select(otherDsp.id);
  const isolated = (await owner.get(statusUrl)).value;
  assert.equal(isolated.requestId, null);
  assert.deepEqual(isolated.paycom.jobs, []);
  assert.deepEqual(isolated.cortex.jobs, []);
});
