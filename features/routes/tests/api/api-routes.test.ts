import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

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
  // Its jobs are Routes' own: listed by its routes, never the timecard's.
  const jobOf = async (id: string) =>
    (await owner.read('/api/dsp/routes/jobs')).find((j: any) => j.id === id);
  await until(async () => {
    const current = await jobOf(job.id);
    assert.notEqual(current.status, 'failed', JSON.stringify(current));
    return current.status === 'succeeded';
  });
  assert.ok(!(await owner.read('/api/dsp/jobs')).some((j: any) => j.id === job.id));
  const finished = await jobOf(job.id);
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
  // One itinerary in full, and a package's history.
  const detail = (await owner.get('/api/dsp/routes/days/2026-09-25/itineraries/itinerary-2')).value;
  assert.equal(detail.itinerary.driverName, 'Second Driver');
  assert.equal(detail.stops.length, 2);
  assert.equal(detail.stops[1].address.address1, '100 Example St');
  assert.deepEqual(
    detail.stops[1].tasks.map((task: any) => [task.trackingId, task.taskState]),
    [
      ['TBA000000000001', 'DELIVERED'],
      ['TBA000000000002', 'UNDELIVERABLE'],
    ],
  );
  assert.equal(detail.breaks.length, 2);
  assert.equal(detail.unknownStops.length, 1);
  assert.equal(detail.inactiveTasks.length, 1);
  assert.equal(
    (await owner.get('/api/dsp/routes/days/2026-09-25/itineraries/missing')).status,
    404,
  );
  const pkg = (await owner.get('/api/dsp/routes/packages/TBA000000000002')).value;
  assert.equal(pkg.trackingId, 'TBA000000000002');
  assert.equal(pkg.events.length, 4);
  assert.ok(pkg.events.every((e: any) => e.day === '2026-09-25'));
  assert.ok(pkg.events.some((e: any) => e.address?.city === 'Fixture'));
  assert.equal((await owner.get('/api/dsp/routes/packages/not%20valid')).status, 400);
  // The platform owner rebuilds the rows from the stored responses.
  const reprocessed = await owner.post(`/api/platform/dsps/${dsp.id}/routes/reprocess`, {
    day: '2026-09-25',
  });
  assert.equal(reprocessed.status, 200, reprocessed.body);
  assert.equal(reprocessed.value.publications.length, 1);
  assert.equal(reprocessed.value.publications[0].taskCount, 8);
  const stored = f.database(`dsps/${dsp.id}/data/routedata/routedata.sqlite`, (db) => ({
    identity: db
      .prepare('SELECT provider,source FROM storage_identity')
      .all()
      .map((r) => ({ ...r })),
    tasks: db
      .prepare(
        "SELECT tracking_id,task_state FROM tasks WHERE itinerary_id='itinerary-2' AND task_type='DROP_OFF' AND active=1 ORDER BY task_id",
      )
      .all()
      .map((r) => ({ ...r })),
    addresses: db.prepare('SELECT count(*) n FROM addresses').get() as { n: number },
    raw: db.prepare('SELECT count(*) n FROM route_raw').get() as { n: number },
  }));
  assert.deepEqual(stored.identity, [{ provider: 'cortex', source: 'routedata-v1' }]);
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
  // A member without Routes' permissions neither reads the days nor collects.
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  assert.equal((await member.get('/api/dsp/routes/days')).status, 403);
  assert.equal((await member.post('/api/dsp/routes/collect', { requestId: 'member' })).status, 403);
  // A routes schedule is its own collection, behind its own routes, and needs the connection.
  const body = {
    name: 'Routes',
    collection: 'routes',
    cadence: 'daily',
    intervalMinutes: null,
    localTime: '05:00',
    enabled: true,
  };
  const refused = await owner.post('/api/dsp/schedules', body);
  assert.equal(refused.status, 403, refused.body);
  assert.equal(refused.value.error, 'permission_denied');
  const schedule = await owner.post('/api/dsp/routes/schedules', body);
  assert.equal(schedule.status, 201, schedule.body);
  assert.equal(schedule.value.collection, 'routes');
  assert.equal(
    (await owner.post('/api/dsp/routes/schedules', { ...body, collection: 'paycom' })).status,
    403,
  );
  assert.ok(
    !(await owner.get('/api/dsp/schedules')).value.schedules.some(
      (s: any) => s.id === schedule.value.id,
    ),
  );
  const scheduled = async () =>
    (await owner.get('/api/dsp/routes/schedules')).value.schedules.find(
      (s: any) => s.id === schedule.value.id,
    );
  assert.equal((await scheduled()).enabled, true);
  assert.equal(
    (await owner.post('/api/dsp/connections/cortex/disable', { removeCredentials: false })).status,
    200,
  );
  assert.equal((await scheduled()).enabled, false);
  // The audit names the collection request.
  const events = (await owner.get('/api/platform/audit?limit=50')).value.events;
  assert.ok(
    events.some(
      (e: any) => e.action === 'routes.collection_requested' && e.detail === '2026-09-25',
    ),
  );
});

test("routes' schedules and jobs stay when the timecard is switched off, and only Routes' own route cancels its job", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  assert.equal((await owner.post('/api/dsp/connections/cortex', cortex)).value.status, 'ready');
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
  const queued = await owner.post('/api/dsp/routes/collect', {
    requestId: 'cancel-me',
    date: '2026-09-25',
  });
  assert.equal(queued.status, 202, queued.body);
  const id = queued.value[0].id;
  const jobOf = async () =>
    (await owner.read('/api/dsp/routes/jobs')).find((j: any) => j.id === id);
  await until(async () => {
    const job = await jobOf();
    assert.notEqual(job.status, 'failed', JSON.stringify(job));
    return job.status === 'succeeded';
  });
  // The job waits again, far off, so a cancel has something to stop; a timecard job sits
  // beside it.
  await f.stop();
  f.database('data/preview/jobs.sqlite', (db) => {
    db.prepare(
      "UPDATE jobs SET status='queued',completed_at=NULL,lease_owner=NULL,lease_until=NULL,available_at=? WHERE id=?",
    ).run(4102444800000, id);
    db.prepare(
      "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,connection_revision,idempotency_key) VALUES ('timecard-job',?,'preview','paycom.collect','succeeded',0,?,'test',1,'timecard-job')",
    ).run(dsp.id, new Date().toISOString());
  });
  await f.start();
  await owner.select(dsp.id);
  assert.equal((await jobOf()).status, 'queued');
  const schedule = await owner.post('/api/dsp/routes/schedules', {
    name: 'Routes',
    collection: 'routes',
    cadence: 'daily',
    intervalMinutes: null,
    localTime: '05:00',
    enabled: true,
  });
  assert.equal(schedule.status, 201, schedule.body);
  const scheduled = async () =>
    (await owner.read('/api/dsp/routes/schedules')).schedules.find(
      (s: any) => s.id === schedule.value.id,
    );
  // The generic route refuses it even to the owner, who holds the timecard's permissions.
  assert.equal((await owner.post(`/api/dsp/jobs/${id}/cancel`, {})).status, 403);
  assert.equal((await jobOf()).status, 'queued');
  // Neither's routes reach the other's: Routes' cancel takes only its own jobs, no schedule
  // moves across, and no preview reads across.
  assert.equal((await owner.post('/api/dsp/routes/jobs/timecard-job/cancel', {})).status, 403);
  assert.ok(!(await owner.read('/api/dsp/routes/jobs')).some((j: any) => j.id === 'timecard-job'));
  assert.ok((await owner.read('/api/dsp/jobs')).some((j: any) => j.id === 'timecard-job'));
  const timing = { cadence: 'daily', intervalMinutes: null, localTime: '06:00' };
  const paycom = await owner.post('/api/dsp/schedules', {
    name: 'Paycom',
    collection: 'paycom',
    enabled: true,
    ...timing,
  });
  assert.equal(paycom.status, 201, paycom.body);
  const move = (url: string, from: any, collection: string) =>
    owner.post(`${url}/${from.id}`, {
      name: from.name,
      collection,
      enabled: true,
      revision: from.revision,
      ...timing,
    });
  assert.equal((await move('/api/dsp/schedules', paycom.value, 'routes')).status, 403);
  assert.equal((await move('/api/dsp/routes/schedules', schedule.value, 'paycom')).status, 403);
  const preview = { scheduleId: schedule.value.id, ...timing };
  assert.equal((await owner.post('/api/dsp/schedules/preview', preview)).status, 403);
  assert.equal((await owner.post('/api/dsp/routes/schedules/preview', preview)).status, 200);

  // The timecard switched off takes its own routes and leaves Routes' schedules and jobs.
  const switches = `/api/platform/dsps/${dsp.id}/features`;
  assert.equal((await owner.post(switches, { feature: 'timecard', enabled: false })).status, 200);
  await owner.select(dsp.id);
  assert.equal((await owner.get('/api/dsp/schedules')).status, 403);
  assert.equal((await owner.get('/api/dsp/routes/schedules')).status, 200);
  assert.equal((await owner.get('/api/dsp/routes/jobs')).status, 200);
  assert.equal((await scheduled()).enabled, true);
  assert.equal((await jobOf()).status, 'queued');

  // Routes' own route cancels it.
  const cancelled = await owner.post(`/api/dsp/routes/jobs/${id}/cancel`, {});
  assert.equal(cancelled.status, 200, cancelled.body);
  assert.equal(cancelled.value.status, 'cancelled');
  assert.equal((await jobOf()).status, 'cancelled');
});

test('route data is kept until the DSP chooses a retention window, which only managers change', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const initial = await owner.get('/api/dsp/routes/retention');
  assert.equal(initial.status, 200, initial.body);
  assert.equal(initial.value.days, null);
  assert.equal(initial.value.storedDays, 0);
  assert.equal(initial.value.oldestDay, null);
  // Windows are 30 days to ten years, or null for every day.
  for (const body of [{ days: 29 }, { days: 3651 }, { days: '90' }, {}, { days: 90, extra: 1 }]) {
    assert.equal((await owner.post('/api/dsp/routes/retention', body)).status, 400);
  }
  const saved = await owner.post('/api/dsp/routes/retention', { days: 90 });
  assert.equal(saved.status, 200, saved.body);
  assert.equal(saved.value.days, 90);
  assert.ok(saved.value.changedAt);
  assert.equal((await owner.get('/api/dsp/routes/retention')).value.days, 90);
  // A day the window has passed is not collected.
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
  const outside = await owner.post('/api/dsp/routes/collect', {
    requestId: 'old',
    date: '2020-01-01',
  });
  assert.equal(outside.status, 400);
  assert.equal(outside.value.error, 'routes_day_outside_retention');
  // A member without the manage permission neither reads nor changes it.
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  assert.equal((await member.get('/api/dsp/routes/retention')).status, 403);
  assert.equal((await member.post('/api/dsp/routes/retention', { days: null })).status, 403);
  assert.equal((await owner.post('/api/dsp/routes/retention', { days: null })).value.days, null);
  const events = (await owner.get('/api/platform/audit?limit=50')).value.events;
  assert.deepEqual(
    events
      .filter((e: any) => e.action === 'routes.retention_changed')
      .map((e: any) => e.detail)
      .sort(),
    ['90', 'forever'],
  );
});
