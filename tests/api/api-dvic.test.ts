import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../support/support.js';

const schedule = {
  name: 'Daily DVIC',
  collection: 'dvic',
  cadence: 'daily',
  intervalMinutes: null,
  localTime: '18:00',
  enabled: true,
};
test('DVIC collects into its own database and schedules run independently of timecards', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const base = '/api/dsp/dvic';
  assert.equal((await owner.post(`${base}/collect`, { requestId: 'unconfigured' })).status, 409);
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(
    (await owner.post(`${base}/schedules`, schedule)).value.error,
    'dvic_station_required',
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
  const roles = (await owner.get('/api/dsp/roles')).value;
  const role = roles.find((r: any) => r.name === 'Member');
  assert.equal(
    (
      await owner.post(`/api/dsp/roles/${role.id}`, {
        name: 'Member',
        permissions: ['dvic.collect', 'dvic.manage'],
      })
    ).status,
    200,
  );
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  assert.equal((await member.get('/api/dsp/jobs')).status, 403);
  for (const bad of [{ week: '2026-W99' }, { weeks: 27 }, { weeks: 0 }, { week: '2999-W01' }]) {
    assert.equal(
      (await member.post(`${base}/collect`, { requestId: 'invalid', ...bad })).status,
      400,
    );
  }
  const queued = await member.post(`${base}/collect`, {
    requestId: 'dvic-test',
    week: '2026-W39',
    weeks: 2,
  });
  assert.equal(queued.status, 202, queued.body);
  assert.equal(queued.value.kind, 'cortex.dvic.collect');
  assert.equal(
    (await member.post(`${base}/collect`, { requestId: 'dvic-test', week: '2026-W39', weeks: 2 }))
      .value.id,
    queued.value.id,
  );
  await until(async () => {
    const job = (await member.get(`${base}/status`)).value.jobs.find(
      (j: any) => j.id === queued.value.id,
    );
    assert.notEqual(job.status, 'failed', JSON.stringify(job));
    return job.status === 'succeeded';
  });
  const status = (await member.get(`${base}/status`)).value;
  assert.equal(status.shortInspections, 2);
  assert.equal(status.reports.length, 2);
  assert.equal(status.jobs[0].metrics.at(-1).rows, 2);
  const rows = (await member.get(`${base}/inspections?from=2026-09-01&to=2026-09-30&limit=1`))
    .value;
  assert.equal(rows.inspections.length, 1);
  assert.equal(rows.inspections[0].durationSeconds, 75);
  assert.equal(rows.inspections[0].minimumSeconds, 90);
  assert.ok(rows.nextCursor);
  assert.equal((await member.get(`${base}/inspections?from=2026-09-30&to=2026-09-01`)).status, 400);
  const created = await member.post(`${base}/schedules`, schedule);
  assert.equal(created.status, 201, created.body);
  const key = created.value.id;
  assert.equal((await member.get(`${base}/schedules`)).value.schedules.length, 1);
  await owner.select(dsp.id);
  assert.equal((await owner.get('/api/dsp/schedules')).value.schedules.length, 0);
  assert.equal(
    (await member.post(`${base}/schedules`, { ...schedule, collection: 'paycom' })).status,
    403,
  );
  assert.equal(
    (
      await member.post(`${base}/schedules/${key}`, {
        ...schedule,
        revision: 1,
        collection: 'routes',
      })
    ).status,
    403,
  );
  const preview = await member.post(`${base}/schedules/preview`, {
    cadence: 'daily',
    intervalMinutes: null,
    localTime: '06:00',
  });
  assert.equal(preview.status, 200, preview.body);
  const changed = await member.post(`${base}/schedules/${key}`, {
    ...schedule,
    cadence: 'interval',
    intervalMinutes: 120,
    revision: 1,
  });
  assert.equal(changed.status, 200, changed.body);
  assert.equal(
    (await member.post(`${base}/schedules/${key}/enabled`, { enabled: false, revision: 1 })).value
      .error,
    'schedule_changed',
  );
  // Timecard switches leave DVIC enabled and its due time intact.
  await owner.post(`/api/platform/dsps/${dsp.id}/features`, {
    feature: 'timecard',
    enabled: false,
  });
  await member.select(dsp.id);
  const independent = (await member.get(`${base}/schedules`)).value.schedules[0];
  assert.equal(independent.enabled, true);
  assert.equal(independent.nextRun, changed.value.nextRun);
  await f.stop();
  f.database(`dsps/${dsp.id}/data/dispatch.sqlite`, (db) =>
    db
      .prepare('UPDATE collection_schedules SET next_run=? WHERE id=?')
      .run('2026-01-01T00:00:00.000Z', key),
  );
  await f.start();
  await until(async () =>
    (await member.get(`${base}/status`)).value.jobs.some(
      (j: any) => j.actorId === null && j.status === 'succeeded',
    ),
  );
  const last = (await member.get(`${base}/schedules`)).value.schedules[0];
  assert.ok(Date.parse(last.nextRun) > Date.now());
  const stored = f.database(`dsps/${dsp.id}/data/dvic/dvic.sqlite`, (db) => ({
    source: db.prepare('SELECT source FROM storage_identity').get() as { source: string },
    count: db.prepare('SELECT count(*) n FROM dvic_runs').get() as { n: number },
  }));
  assert.equal(stored.source.source, 'dvic-v1');
  assert.equal(stored.count.n, 2);
  await owner.select(dsp.id);
  const other = owner.session.dsps.find((d: any) => d.permanent);
  await owner.select(other.id);
  assert.equal((await owner.get(`${base}/status`)).value.shortInspections, 0);
  assert.equal((await owner.post(`${base}/jobs/${queued.value.id}/cancel`, {})).status, 404);
  await owner.select(dsp.id);
  await owner.post('/api/dsp/connections/cortex/disable', { removeCredentials: false });
  assert.equal((await member.get(`${base}/schedules`)).value.schedules[0].enabled, false);
  assert.equal((await member.post(`${base}/collect`, { requestId: 'disconnected' })).status, 409);
  const current = (await member.get(`${base}/schedules`)).value.schedules[0];
  assert.equal(
    (await member.post(`${base}/schedules/${key}/remove`, { revision: current.revision })).status,
    200,
  );
});
