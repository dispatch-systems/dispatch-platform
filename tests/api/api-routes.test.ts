import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../support/support.js';

const cortex = { username: 'fixture@example.test', password: 'fixture-password' };

test('a day of routes is collected on request, stored in normalized rows and listed, and a routes schedule needs a station', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  assert.equal((await owner.post('/api/dsp/connections/cortex', cortex)).value.status, 'ready');
  // The days view starts empty and names the day a collection targets by default.
  const before = (await owner.get('/api/dsp/routes/days')).value;
  assert.deepEqual(before.days, []);
  assert.match(before.latestDay, /^\d{4}-\d{2}-\d{2}$/);
  // Without a station nothing can be queued.
  assert.equal(
    (await owner.post('/api/dsp/routes/collect', { requestId: 'no-station' })).status,
    409,
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
  assert.equal(
    (await owner.post('/api/dsp/routes/collect', { requestId: 'bad', date: '2026-13-01' })).status,
    400,
  );
  assert.equal(
    (
      await owner.post('/api/dsp/routes/collect', {
        requestId: 'today',
        date: before.latestDay,
        mode: 'snapshot',
      })
    ).status,
    400,
  );
  const queued = await owner.post('/api/dsp/routes/collect', {
    requestId: 'day-25',
    date: '2026-09-25',
  });
  assert.equal(queued.status, 202, queued.body);
  assert.equal(queued.value.length, 1);
  const job = queued.value[0];
  assert.equal(job.kind, 'cortex.routes.collect');
  assert.equal(
    (await owner.post('/api/dsp/routes/collect', { requestId: 'day-25', date: '2026-09-25' }))
      .value[0].id,
    job.id,
  );
  await until(async () => {
    const current = (await owner.get('/api/dsp/jobs')).value.find((j: any) => j.id === job.id);
    assert.notEqual(current.status, 'failed', JSON.stringify(current));
    return current.status === 'succeeded';
  });
  const finished = (await owner.get('/api/dsp/jobs')).value.find((j: any) => j.id === job.id);
  assert.equal(finished.metrics.at(-1).itineraries, 2);
  const days = (await owner.get('/api/dsp/routes/days')).value;
  assert.equal(days.days.length, 1);
  assert.equal(days.days[0].day, '2026-09-25');
  assert.equal(days.days[0].mode, 'final');
  assert.equal(days.days[0].itineraryCount, 2);
  assert.equal(days.days[0].taskCount, 8);
  const day = (await owner.get('/api/dsp/routes/days/2026-09-25')).value;
  assert.equal(day.publication.id, days.days[0].id);
  assert.equal(day.itineraries.length, 2);
  assert.equal(day.itineraries[0].driverName, 'Fixture Driver');
  assert.equal(day.itineraries[0].delivered, 2);
  assert.equal(day.itineraries[1].notDelivered, 1);
  assert.equal((await owner.get('/api/dsp/routes/days/2026-09-24')).status, 404);
  const stored = f.database(`dsps/${dsp.id}/data/itineraries/itineraries.sqlite`, (db) => ({
    identity: db
      .prepare('SELECT provider,source FROM storage_identity')
      .all()
      .map((r) => ({ ...r })),
    tasks: db
      .prepare(
        "SELECT tracking_id,task_state FROM tasks WHERE itinerary_id='itinerary-2' AND task_type='DROP_OFF' ORDER BY task_id",
      )
      .all()
      .map((r) => ({ ...r })),
    addresses: db.prepare('SELECT count(*) n FROM addresses').get() as { n: number },
    raw: db.prepare('SELECT count(*) n FROM route_raw').get() as { n: number },
  }));
  assert.deepEqual(stored.identity, [{ provider: 'cortex', source: 'itineraries-v1' }]);
  assert.deepEqual(stored.tasks, [
    { tracking_id: 'TBA000000000001', task_state: 'DELIVERED' },
    { tracking_id: 'TBA000000000002', task_state: 'UNDELIVERABLE' },
  ]);
  assert.equal(stored.addresses.n, 2);
  assert.equal(stored.raw.n, 4);
  // Several days queue one job each.
  const range = await owner.post('/api/dsp/routes/collect', {
    requestId: 'range',
    date: '2026-09-20',
    days: 3,
  });
  assert.equal(range.status, 202, range.body);
  assert.equal(range.value.length, 3);
  // A member without the collect permission can read the days but not collect.
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  assert.equal((await member.get('/api/dsp/routes/days')).status, 403);
  assert.equal((await member.post('/api/dsp/routes/collect', { requestId: 'member' })).status, 403);
  // A routes schedule is its own collection and needs the connection.
  const schedule = await owner.post('/api/dsp/schedules', {
    name: 'Routes',
    collection: 'routes',
    cadence: 'daily',
    intervalMinutes: null,
    localTime: '05:00',
    enabled: true,
  });
  assert.equal(schedule.status, 201, schedule.body);
  assert.equal(schedule.value.collection, 'routes');
  assert.equal(
    (await owner.post('/api/dsp/connections/cortex/disable', { removeCredentials: false })).status,
    200,
  );
  const rows = (await owner.get('/api/dsp/schedules')).value.schedules;
  assert.equal(rows.find((s: any) => s.id === schedule.value.id).enabled, false);
  // The audit names the collection request.
  const events = (await owner.get('/api/platform/audit?limit=50')).value.events;
  assert.ok(
    events.some(
      (e: any) => e.action === 'routes.collection_requested' && e.detail === '2026-09-25',
    ),
  );
});
