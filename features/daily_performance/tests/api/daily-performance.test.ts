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
  // Both days are one job.
  assert.equal(queued.value.length, 1);
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
  // Its policy is its Settings section, each value within its range, and each DSP's own.
  const policy = '/api/dsp/settings/daily_performance.collection';
  const section = async (client: typeof owner) =>
    (await client.read('/api/dsp/settings')).sections.find(
      (each: { id: string }) => each.id === 'daily_performance.collection',
    );
  const defaults = { lookback_days: 7, refresh_hours: 20 };
  assert.deepEqual((await section(owner)).values, defaults);
  for (const values of [
    { lookback_days: 0 },
    { lookback_days: 31 },
    { refresh_hours: 0 },
    { refresh_hours: 169 },
    { lookback_days: 2.5 },
  ]) {
    const refused = await owner.post(policy, { values, revision: 0 });
    assert.equal(refused.value.error, 'invalid_input', JSON.stringify(values));
  }
  const saved = await owner.post(policy, {
    values: { lookback_days: 3, refresh_hours: 12 },
    revision: 0,
  });
  assert.equal(saved.status, 200, saved.body);
  assert.deepEqual((await section(owner)).values, { lookback_days: 3, refresh_hours: 12 });
  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(summit.id);
  assert.deepEqual((await section(owner)).values, defaults);
  await owner.select(dsp.id);
  // Its schedule reads Daily Performance alone, never the weekly scorecard beside it.
  const daily = (collects: string) => ({
    id: null,
    collects,
    kind: 'times',
    days: [0, 1, 2, 3, 4, 5, 6],
    times: ['07:00'],
  });
  const collected = (await owner.read('/api/dsp/collectors')).collections.find(
    (each: { collection: string }) => each.collection === 'daily_performance',
  );
  const schedule = async (collects: string) =>
    owner.post('/api/dsp/collectors/daily_performance/schedule', {
      rules: [daily(collects)],
      revision: collected.revision,
    });
  assert.equal((await schedule('weekly_scorecard')).value.error, 'invalid_input');
  const scheduled = await schedule('daily_performance');
  assert.equal(scheduled.status, 200, scheduled.body);
  assert.deepEqual(
    scheduled.value.rules.map((rule: { collects: string }) => rule.collects),
    ['daily_performance'],
  );
  const member = await f.client(demo.member);
  await member.select(dsp.id);
  assert.equal((await member.get(prefix)).status, 403);
  assert.equal(
    (await member.post(prefix + '/collect', { requestId: 'member', date: '2026-10-03' })).status,
    403,
  );
  assert.equal(await section(member), undefined);
  assert.equal(
    (await member.post(policy, { values: { lookback_days: 1 }, revision: 1 })).status,
    403,
  );
});
