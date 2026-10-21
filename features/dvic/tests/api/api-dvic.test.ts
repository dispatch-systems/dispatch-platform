import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import { seedDvic } from '../support/dvic.js';
import { execFileSync } from 'node:child_process';

const daily = {
  id: null,
  collects: 'dvic',
  kind: 'times',
  days: [0, 1, 2, 3, 4, 5, 6],
  times: ['18:00'],
};
type Collection = {
  collection: string;
  enabled: boolean;
  revision: string;
  nextRun: string | null;
  rules: any[];
};

test('DVIC summaries and pages keep complete filtered counts, stable cells and fresh operator exclusions', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const { owner, dsp } = await seedDvic(f, 700);
  f.database(`dsps/${dsp.id}/data/dvic/dvic.sqlite`, (db) => {
    db.exec('BEGIN');
    const insert = db.prepare(`INSERT INTO dvic_inspections
      SELECT company_id,?,dsp_code,?,start_date,?,?,vin,?,inspection_type,inspection_status,
        ?,end_time,?,?,?,report_date,source_modified_at,revision_id,?
      FROM dvic_inspections WHERE inspection_key='page-row-0'`);
    for (let i = 0; i < 700; i++) {
      const step = i >= 1 && i <= 3;
      const time = i === 2 || i === 3 ? '06:00:00' : i === 699 ? '12:00:00' : '09:00:00';
      insert.run(
        `extra-${String(i).padStart(4, '0')}`,
        'TST1',
        'EXTRA1',
        i === 699 ? 'Zoë Changed' : 'Extra Driver',
        step ? 'SV' : 'CV',
        `2026-09-26T${time}`,
        step ? 200 : i === 0 ? 80 : 85,
        step ? 300 : 90,
        1,
        1,
      );
    }
    for (let i = 0; i < 27; i++)
      insert.run(
        `driver-${i}`,
        'TST1',
        `MORE${i}`,
        i === 0 ? 'Åke Example' : i === 1 ? 'Zoë Example' : `Driver ${String(i).padStart(2, '0')}`,
        'CV',
        '2026-09-26T09:00:00',
        30,
        90,
        1,
        1,
      );
    for (const [key, station, short, verified] of [
      ['foreign', 'OTHER', 1, 1],
      ['unverified', 'TST1', 1, 0],
      ['corrected', 'TST1', 0, 1],
    ] as const)
      insert.run(
        key,
        station,
        'EXCLUDED',
        'Excluded',
        'CV',
        '2026-09-26T09:00:00',
        30,
        90,
        short,
        verified,
      );
    db.exec('COMMIT');
  });
  const role = (await owner.read('/api/dsp/roles')).find((role: any) => role.name === 'Member');
  assert.equal(
    (await owner.post(`/api/dsp/roles/${role.id}`, { name: 'Member', permissions: ['dvic.view'] }))
      .status,
    200,
  );
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  const base = '/api/dsp/dvic';
  const week = 'from=2026-09-20&to=2026-09-26';
  const summary = await member.read(`${base}/summary?${week}`);
  assert.equal(summary.totalInspections, 1427);
  assert.equal(summary.totalDrivers, 36);
  assert.equal(summary.drivers.length, 25);
  assert.equal(summary.drivers[0].driverId, 'EXTRA1');
  assert.equal(summary.drivers[0].driverName, 'Zoë Changed');
  assert.deepEqual(summary.drivers[0].fleets, ['CV', 'SV']);
  assert.deepEqual(summary.drivers[0].minimums, [90, 300]);
  assert.equal(summary.drivers[0].count, 700);
  assert.deepEqual(summary.drivers[0].cells, [
    {
      id: 'company-fixture:extra-0002',
      startDate: '2026-09-26',
      durationSeconds: 200,
      minimumSeconds: 300,
    },
  ]);
  const next = await member.read(`${base}/summary?${week}&offset=25`);
  assert.equal(next.drivers.length, 11);
  assert.equal(next.drivers.at(-1).driverName, 'Zoë Example');
  assert.equal(
    new Set([...summary.drivers, ...next.drivers].map((driver: any) => driver.driverId)).size,
    36,
  );
  assert.equal(summary.days.find((day: any) => day.date === '2026-09-26').count, 827);
  assert.equal(
    summary.days.find((day: any) => day.date === '2026-09-26').repeatDrivers[0],
    'Zoë Changed',
  );
  const dayUrl = `${base}/day?date=2026-09-26&vehicles=non-dot`;
  const day = await member.read(dayUrl);
  assert.equal(day.total, 791);
  assert.equal(day.inspections.length, 100);
  assert.equal(day.minimum, 90);
  const dayNext = await member.read(dayUrl + '&offset=100');
  assert.equal(dayNext.inspections.length, 100);
  assert.ok(!day.inspections.some((row: any) => row.id === dayNext.inspections[0].id));
  const unicode = await member.read(
    `${base}/summary?${week}&q=${encodeURIComponent('  ZOË CHANGED  ')}`,
  );
  assert.equal(unicode.totalInspections, 1);
  assert.equal(unicode.drivers[0].driverId, 'EXTRA1');
  assert.equal(
    (await member.read(`${base}/summary?${week}&q=FIXTURE`)).totalInspections,
    0,
    'VINs are not searched',
  );
  const dot = await member.read(`${base}/summary?${week}&q=extra1&vehicles=dot`);
  assert.equal(dot.totalInspections, 3);
  assert.deepEqual(dot.drivers[0].minimums, [300]);
  assert.deepEqual(dot.days[0].repeatDrivers, ['Extra Driver']);
  const detailUrl = `${base}/inspection?${week}&id=company-fixture:extra-0002`;
  const detail = await member.read(detailUrl);
  assert.equal(detail.inspection.vin, '1FIXTURE000000000');
  assert.equal(detail.relatedTotal, 699);
  assert.equal(detail.related.length, 50);
  assert.equal(detail.related[0].id, 'company-fixture:extra-0699');
  for (const url of [
    `${base}/summary?from=2026-09-01&to=2026-09-30`,
    `${base}/summary?${week}&vehicles=other`,
    dayUrl + '&offset=-1',
  ])
    assert.equal((await member.get(url)).status, 400);
  for (const id of [
    'company-fixture:foreign',
    'company-fixture:unverified',
    'company-fixture:corrected',
  ])
    assert.equal((await member.get(`${base}/inspection?${week}&id=${id}`)).status, 404);
  // The actual operator command writes from outside the running server. Warm
  // summaries, day pages and detail must all stop answering the removed records.
  execFileSync(
    process.env.DISPATCH_TEST_BINARY ?? 'target/debug/dispatch-backend',
    ['dvic-hide', dsp.id, 'EXTRA1', 'Fixture exclusion'],
    { env: f.env, stdio: 'pipe' },
  );
  assert.equal((await member.read(`${base}/summary?${week}`)).totalInspections, 727);
  assert.equal((await member.read(dayUrl)).total, 94);
  assert.equal((await member.get(detailUrl)).status, 404);
  const other = owner.session.dsps.find((value: any) => value.name === 'Summit Delivery');
  await owner.select(other.id);
  const isolated = await owner.get(`${base}/summary?${week}`);
  assert.equal(isolated.status, 200, isolated.body);
  assert.equal(isolated.value.totalInspections, 0);
  assert.equal((await owner.get(detailUrl)).status, 404);
  await owner.select(dsp.id);
  assert.equal(
    (await owner.post(`/api/dsp/roles/${role.id}`, { name: 'Member', permissions: [] })).status,
    200,
  );
  await member.select(dsp.id);
  assert.equal((await member.get(`${base}/summary?${week}`)).status, 403);
});
test('DVIC collects into its own database and schedules run independently of timecards', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const base = '/api/dsp/dvic';
  // Its schedule, on the Collectors page: as a member reads it, and saved over what they read.
  const collectors = '/api/dsp/collectors';
  const dvic = async (client: typeof owner): Promise<Collection> =>
    (await client.read(collectors)).collections.find(
      (each: Collection) => each.collection === 'dvic',
    );
  const schedule = async (client: typeof owner, rules: unknown[]) =>
    client.post(`${collectors}/dvic/schedule`, { rules, revision: (await dvic(client)).revision });
  assert.equal((await owner.post(`${base}/collect`, { requestId: 'unconfigured' })).status, 409);
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  // Saved before the DSP has a station, it waits, off, until it has one.
  const waiting = await schedule(owner, [daily]);
  assert.equal(waiting.status, 200, waiting.body);
  assert.equal(waiting.value.enabled, false);
  const on = async (client: typeof owner) =>
    client.post(`${collectors}/dvic/enabled`, {
      enabled: true,
      revision: (await dvic(client)).revision,
    });
  assert.equal((await on(owner)).value.error, 'dvic_station_required');
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
    const job = (await member.read(`${base}/status`)).jobs.find(
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
  // DVIC's own permission sets its schedule, and no other collection's.
  assert.deepEqual(
    (await member.read(collectors)).collections.map((each: Collection) => each.collection),
    ['dvic'],
  );
  const created = await schedule(member, [daily]);
  assert.equal(created.status, 200, created.body);
  const key = created.value.rules[0].id;
  const resumed = await on(member);
  assert.equal(resumed.status, 200, resumed.body);
  assert.equal(resumed.value.enabled, true);
  await owner.select(dsp.id);
  const paycom = (await owner.read(collectors)).collections.find(
    (each: Collection) => each.collection === 'paycom',
  );
  assert.equal(
    (
      await member.post(`${collectors}/paycom/schedule`, {
        rules: [{ ...daily, collects: 'paycom' }],
        revision: paycom.revision,
      })
    ).status,
    403,
  );
  assert.equal(
    (await schedule(member, [{ ...daily, id: key, collects: 'routes' }])).value.error,
    'invalid_input',
  );
  const repeat = { ...daily, id: key, kind: 'repeat', every: 120, from: '00:00', to: '00:00' };
  const preview = await member.post(`${collectors}/dvic/preview`, { rules: [repeat] });
  assert.equal(preview.status, 200, preview.body);
  const changed = await schedule(member, [repeat]);
  assert.equal(changed.status, 200, changed.body);
  assert.deepEqual(changed.value.rules[0].id, key);
  // Timecard switches leave DVIC enabled and its due time intact.
  await owner.post(`/api/platform/dsps/${dsp.id}/features`, {
    feature: 'timecard',
    enabled: false,
  });
  await member.select(dsp.id);
  const independent = await dvic(member);
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
    (await member.read(`${base}/status`)).jobs.some(
      (j: any) => j.actorId === null && j.status === 'succeeded',
    ),
  );
  const last = (await dvic(member)).rules[0];
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
  assert.equal((await dvic(member)).enabled, false);
  assert.equal((await member.post(`${base}/collect`, { requestId: 'disconnected' })).status, 409);
  const removed = await schedule(member, []);
  assert.equal(removed.status, 200, removed.body);
  assert.deepEqual(removed.value.rules, []);
});

test("the generic cancel route never cancels a DVIC job, only DVIC's own route does; switching DVIC off drops what its jobs kept", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  await owner.post('/api/dsp/profile', {
    name: dsp.name,
    abbreviation: 'NLL',
    stationCode: 'TST1',
    timezone: dsp.timezone,
  });
  await owner.select(dsp.id);
  const queued = await owner.post('/api/dsp/dvic/collect', {
    requestId: 'cancel-permission',
    week: '2026-W39',
  });
  assert.equal(queued.status, 202, queued.body);
  const id = queued.value.id;
  await until(async () => {
    const job = (await owner.read('/api/dsp/dvic/status')).jobs.find((j: any) => j.id === id);
    assert.notEqual(job?.status, 'failed', JSON.stringify(job));
    return job?.status === 'succeeded';
  });
  // collections.run cancels timecard jobs, never a job DVIC owns.
  const role = (await owner.get('/api/dsp/roles')).value.find((r: any) => r.name === 'Member');
  const grant = async (permissions: string[]) => {
    await owner.select(dsp.id);
    assert.equal(
      (await owner.post(`/api/dsp/roles/${role.id}`, { name: 'Member', permissions })).status,
      200,
    );
  };
  await grant(['collections.run']);
  const member = await f.client('member@dispatch.test');
  await member.select(dsp.id);
  {
    const reply = await member.post(`/api/dsp/jobs/${id}/cancel`, {});
    assert.equal(reply.status, 403, reply.body);
  }
  {
    const reply = await member.post(`/api/dsp/dvic/jobs/${id}/cancel`, {});
    assert.equal(reply.status, 403, reply.body);
  }
  // With DVIC's own permission too, only DVIC's route cancels it; the generic route
  // refuses it even to the owner, who holds every permission.
  await grant(['collections.run', 'dvic.collect']);
  await member.select(dsp.id);
  {
    const reply = await member.post(`/api/dsp/jobs/${id}/cancel`, {});
    assert.equal(reply.status, 403, reply.body);
  }
  {
    const reply = await member.post(`/api/dsp/dvic/jobs/${id}/cancel`, {});
    assert.equal(reply.status, 200, reply.body);
  }
  await owner.select(dsp.id);
  {
    const reply = await owner.post(`/api/dsp/jobs/${id}/cancel`, {});
    assert.equal(reply.status, 403, reply.body);
  }
  // Switching the page off cancels its waiting job and drops the live run it kept,
  // which a worker's own finish would no longer do once the lease is gone.
  await f.stop();
  f.database('data/preview/jobs.sqlite', (db) =>
    db
      .prepare(
        "UPDATE jobs SET status='queued',completed_at=NULL,lease_owner=NULL,lease_until=NULL,available_at=? WHERE id=?",
      )
      .run(4102444800000, id),
  );
  f.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) =>
    db.prepare('INSERT INTO collection_live_runs VALUES (?,?,?)').run(id, 'worker-1', '{}'),
  );
  await f.start();
  await owner.select(dsp.id);
  // DVIC's status lists its jobs; the generic list never does.
  const jobOf = async () =>
    (await owner.read('/api/dsp/dvic/status')).jobs.find((j: any) => j.id === id);
  assert.equal((await jobOf()).status, 'queued');
  assert.ok(!(await owner.read('/api/dsp/jobs')).some((j: any) => j.id === id));
  const switches = `/api/platform/dsps/${dsp.id}/features`;
  assert.equal((await owner.post(switches, { feature: 'dvic', enabled: false })).status, 200);
  assert.equal(
    f.database(
      `dsps/${dsp.id}/data/cortex/cortex.sqlite`,
      (db) => (db.prepare('SELECT count(*) n FROM collection_live_runs').get() as { n: number }).n,
    ),
    0,
  );
  // Switched back on, its status shows the job the switch cancelled.
  assert.equal((await owner.post(switches, { feature: 'dvic', enabled: true })).status, 200);
  await owner.select(dsp.id);
  assert.equal((await jobOf()).status, 'cancelled');
  assert.ok(!(await owner.read('/api/dsp/jobs')).some((j: any) => j.id === id));
});
