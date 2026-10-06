import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';

test('daily collection, policy and reads stay separate from weekly data and permissions', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  assert.equal(
    (
      await owner.post('/api/dsp/connections/cortex', {
        username: 'fixture@example.test',
        password: 'fixture-password',
      })
    ).value.status,
    'ready',
  );
  assert.equal(
    (
      await owner.post('/api/dsp/profile', {
        name: dsp.name,
        abbreviation: 'NLL',
        stationCode: 'TST1',
        timezone: dsp.timezone,
      })
    ).status,
    200,
  );
  await owner.select(dsp.id);
  const prefix = '/api/dsp/daily-performance';
  assert.deepEqual((await owner.get(prefix)).value.days, []);
  const queued = await owner.post(prefix + '/collect', {
    requestId: 'two-days',
    from: '2026-10-03',
    to: '2026-10-04',
  });
  assert.equal(queued.status, 202, queued.body);
  assert.equal(queued.value.length, 2);
  assert.ok(queued.value.every((job: any) => job.kind === 'cortex.daily_performance.collect'));
  await until(async () => {
    const jobs = (await owner.get(prefix + '/jobs')).value;
    assert.ok(
      jobs.every((job: any) => job.status !== 'failed'),
      JSON.stringify(jobs),
    );
    return queued.value.every(
      (queued: any) => jobs.find((job: any) => job.id === queued.id)?.status === 'succeeded',
    );
  });
  const summary = (await owner.get(prefix)).value;
  assert.deepEqual(
    summary.days.map((day: any) => day.date),
    ['2026-10-04', '2026-10-03'],
  );
  assert.equal(summary.days[0].datasets.length, 22);
  assert.equal(
    summary.days[0].datasets.find((dataset: any) => dataset.dataset === 'driver_contacts').coverage,
    'unconfirmed',
  );
  assert.deepEqual((await owner.get('/api/dsp/weekly-scorecard/weeks')).value.weeks, []);
  assert.deepEqual((await owner.get('/api/dsp/weekly-scorecard/jobs')).value, []);
  const policy = await owner.post(prefix + '/policy', { lookbackDays: 3, refreshHours: 12 });
  assert.equal(policy.status, 200, policy.body);
  assert.deepEqual((await owner.get(prefix + '/policy')).value, {
    lookbackDays: 3,
    refreshHours: 12,
  });
  assert.equal(
    (await owner.post(prefix + '/policy', { lookbackDays: 0, refreshHours: 12 })).status,
    400,
  );
  const schedule = await owner.post(prefix + '/schedules', {
    name: 'Daily Quality',
    collection: 'daily_performance',
    cadence: 'daily',
    intervalMinutes: null,
    localTime: '07:00',
    enabled: true,
  });
  assert.equal(schedule.status, 201, schedule.body);
  assert.equal(schedule.value.collection, 'daily_performance');
  assert.equal(
    (
      await owner.post(prefix + '/schedules', {
        name: 'Wrong source',
        collection: 'weekly_scorecard',
        cadence: 'daily',
        intervalMinutes: null,
        localTime: '07:00',
        enabled: true,
      })
    ).status,
    403,
  );
  const member = await f.client(demo.member);
  await member.select(dsp.id);
  assert.equal((await member.get(prefix)).status, 403);
  assert.equal(
    (await member.post(prefix + '/collect', { requestId: 'member', date: '2026-10-03' })).status,
    403,
  );
  const key = await owner.post('/api/platform/agents/keys', {
    name: 'Daily only',
    allDsps: false,
    dsps: [dsp.id],
    access: 'read',
    reads: { areas: ['daily_performance', 'daily_returns', 'daily_safety'], bypass: false },
    dspReads: [],
    expiresAt: null,
  });
  assert.equal(key.status, 200, key.body);
  const headers = { authorization: `Bearer ${key.value.token}` };
  const status = await f.request('/api/v1/status', undefined, headers);
  assert.equal(status.status, 200, status.body);
  assert.equal(status.value.sources.daily_performance.latestDay, '2026-10-04');
  assert.equal(status.value.sources.weekly_scorecard.reads, false);
  const one = await f.request(
    '/api/v1/daily-performance?date=2026-10-03&driver=driver-2&detail=full&fields=delivered',
    undefined,
    headers,
  );
  assert.equal(one.status, 200, one.body);
  assert.equal(one.value.recorded_rows, 1);
  assert.equal(one.value.coverage.status, 'complete');
  assert.equal(one.value.list.rows.length, 1);
  const partial = await f.request(
    '/api/v1/daily-performance?from=2026-10-02&to=2026-10-04',
    undefined,
    headers,
  );
  assert.equal(partial.status, 200, partial.body);
  assert.equal(partial.value.coverage.status, 'partial');
  assert.equal(partial.value.coverage.observed_days, 2);
  assert.equal(partial.value.coverage.requested_days, 3);
  const returns = await f.request(
    '/api/v1/returns?source=daily_performance&date=2026-10-03&reason=business_closed&impacting=true&list=true',
    undefined,
    headers,
  );
  assert.equal(returns.status, 200, returns.body);
  assert.equal(returns.value.recorded_rows, 1);
  assert.equal(returns.value.source, 'daily_performance');
  assert.equal(
    (await f.request('/api/v1/returns?source=weekly_scorecard&date=2026-10-03', undefined, headers))
      .status,
    403,
  );
  const daily = await f.request(
    '/api/v1/safety?source=daily_performance&date=2026-10-04&list=true',
    undefined,
    headers,
  );
  assert.equal(daily.status, 200, daily.body);
  assert.equal(daily.value.recorded_rows, 1);
  assert.equal(daily.value.dataset, 'safety_events');
  const switches = `/api/platform/dsps/${dsp.id}/features`;
  await owner.post(switches, { feature: 'weekly_scorecard', enabled: false });
  assert.equal(
    (await f.request('/api/v1/daily-performance?date=2026-10-03', undefined, headers)).status,
    200,
  );
  await owner.post(switches, { feature: 'daily_performance', enabled: false });
  assert.equal(
    (await f.request('/api/v1/daily-performance?date=2026-10-03', undefined, headers)).status,
    403,
  );
});
