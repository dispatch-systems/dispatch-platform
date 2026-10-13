import test from 'node:test';
import assert from 'node:assert/strict';
import type { MealEmployee } from '../../api/index.js';
import type { Driver, DriverMatch } from '../../../driver_match/api/index.js';
import { checkReply } from '../../../../core/foundation/api/runtime.js';
import { replies as timecardReplies } from '../../api/runtime.js';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

test('meal API requires DSP context, exposes the punch union to members, and links only through Driver Match', async () => {
  const f = await fixture();
  try {
    const owner = await f.client();
    const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
    await owner.select(dsp.id);
    const date = f.collector(
      dsp.id,
      (db) => (db.prepare('SELECT max(date) date FROM timecards').get() as { date: string }).date,
    );
    const response = await owner.get(`/api/dsp/paycom/meal-breaks?date=${date}`);
    assert.equal(response.status, 200);
    assert.equal(response.value.rows.length, 12);
    const pagePath = '/api/dsp/paycom/meal-breaks/page';
    const first = await owner.get(`${pagePath}?date=${date}&limit=3&offset=0`);
    checkReply([timecardReplies], pagePath, 'GET', first.value);
    assert.equal(first.value.total, 12);
    assert.equal(first.value.counts.all, 12);
    assert.deepEqual(first.value.rows, response.value.rows.slice(0, 3));
    const second = await owner.get(`${pagePath}?date=${date}&limit=3&offset=3`);
    assert.deepEqual(second.value.rows, response.value.rows.slice(3, 6));
    const late = response.value.rows.filter((row: MealEmployee) => row.assessment.lateIn);
    const latePage = await owner.get(`${pagePath}?date=${date}&limit=all&filter=late`);
    assert.deepEqual(latePage.value.rows, late);
    assert.equal(latePage.value.counts.late, late.length);
    const searched = await owner.get(`${pagePath}?date=${date}&q=E001&limit=all`);
    assert.deepEqual(
      searched.value.rows.map((row: MealEmployee) => row.id),
      ['paycom:E001'],
    );
    const descending = await owner.get(`${pagePath}?date=${date}&limit=all&direction=desc`);
    assert.deepEqual(descending.value.rows, [...response.value.rows].reverse());
    for (const query of ['limit=101', 'sort=unknown', 'filter=unknown'])
      assert.equal((await owner.get(`${pagePath}?date=${date}&${query}`)).status, 400);
    // The Late DAs filter reads each card's department.
    assert.deepEqual(
      [...new Set(response.value.rows.map((r: MealEmployee) => r.paycom?.department))].sort(),
      ['Delivery', 'Operations'],
    );
    assert.equal((await owner.get('/api/dsp/paycom/meal-breaks?date=invalid')).status, 400);
    const member = await f.client('member@dispatch.test');
    assert.equal((await member.get(`${pagePath}?date=${date}`)).status, 403);
    await member.select(dsp.id);
    assert.equal((await member.get(`${pagePath}?date=${date}&limit=2`)).value.rows.length, 2);
    assert.equal((await member.get(`/api/dsp/paycom/meal-breaks?date=${date}`)).status, 200);
    const periodUrl = `/api/dsp/paycom/meal-breaks/period?source=paycom%3AE001&date=${date}`;
    const period = await member.get(periodUrl);
    assert.equal(period.status, 200);
    checkReply([timecardReplies], '/api/dsp/paycom/meal-breaks/period', 'GET', period.value);
    assert.equal(period.value.days.length, 14);
    assert(
      period.value.days.every((day: any) =>
        day.rows.every((row: MealEmployee) => row.id === 'paycom:E001'),
      ),
    );
    const employee = await member.get(`/api/dsp/employees/E001?date=${date}`);
    assert.deepEqual(period.value.period, employee.value.period);
    // Everyone's counts read the same period, so the page's count matches the summary's.
    const gapsPath = '/api/dsp/paycom/meal-breaks/period-gaps';
    const gaps = await member.get(`${gapsPath}?date=${date}`);
    assert.equal(gaps.status, 200);
    checkReply([timecardReplies], gapsPath, 'GET', gaps.value);
    assert.deepEqual(gaps.value.period, period.value.period);
    assert.equal(gaps.value.counts['paycom:E001'] ?? 0, period.value.gaps.length);
    assert.equal((await member.get(`${periodUrl}&from=2026-09-06&to=2026-09-19`)).status, 400);
    assert.equal(
      (await member.get('/api/dsp/paycom/meal-breaks/period?source=unknown&date=2026-09-07'))
        .status,
      400,
    );
    assert.equal(
      (
        await member.get(
          '/api/dsp/paycom/meal-breaks/period?source=paycom%3AE001&from=2026-09-07&to=2026-09-20',
        )
      ).status,
      400,
    );
    // The meal-break page's own link editor is gone; links are decided in Driver Match.
    const retired = { revision: 0, changes: [{ cortexId: 'driver-1', paycomCode: null }] };
    assert.equal((await member.post('/api/dsp/paycom/employee-links', retired)).status, 403);
    assert.equal((await owner.post('/api/dsp/paycom/employee-links', retired)).status, 404);
  } finally {
    await f.close();
  }
});

test('name variants combine existing source records without recollection or losing punches and meals', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const names = [
    ['REED, JAMIE', 'Jamie Reed Vega'],
    ['STONE, ALEX', 'Alexander Stone'],
    ['MOLINA, CASEY', 'Casey Molina Solis'],
    ['HART, TAYLOR', 'Taylor Hart Jr'],
  ];
  const date = f.collector(dsp.id, (db) => {
    const date = (db.prepare('SELECT max(date) date FROM timecards').get() as { date: string })
      .date;
    names.forEach(([paycom], i) => {
      db.prepare('UPDATE employees SET name=? WHERE code=?').run(paycom!, `E00${i + 1}`);
    });
    return date;
  });
  const endpoint = `/api/dsp/paycom/meal-breaks?date=${date}`;
  const before = (await owner.get(endpoint)).value;
  // Fixture writes bypass the collector's revision notification; restart after seeding.
  await f.stop();
  f.database(`dsps/${dsp.id}/data/cortex/cortex.sqlite`, (db) => {
    db.prepare(
      `INSERT INTO meal_publications VALUES
       ('variant-pub','variant-job',?,'DEMO1','area-1','ALL_DRIVERS','UTC',?,?,1,4,5,5,3)`,
    ).run(date, `${date}T23:00:00Z`, `${date}T23:01:00Z`);
    names.forEach(([, flex], i) => {
      db.prepare(
        `INSERT INTO meal_itineraries VALUES
         ('variant-pub',?,?,?,'R1',?,1,'complete','recorded')`,
      ).run(`route-${i}`, `driver-${i}`, flex!, `${date}T23:00:00Z`);
      for (const meal of i === 0 ? [0, 1] : [0]) {
        const hour = 12 + meal * 4;
        db.prepare(
          `INSERT INTO meal_records VALUES ('variant-pub',?,?,?,?,?,?,'verified','verified')`,
        ).run(
          `route-${i}`,
          `meal-${meal}`,
          `${date}T${hour - 1}:58:00Z`,
          `${date}T${hour}:00:00Z`,
          `${date}T${hour}:30:00Z`,
          `${date}T${hour}:32:00Z`,
        );
      }
    });
  });
  await f.start();
  const response = await owner.get(endpoint);
  assert.equal(response.status, 200);
  const combined = response.value;
  assert.equal(combined.rows.length, before.rows.length);
  assert.equal(combined.drivers.length, 4);
  assert(combined.drivers.every((d: any) => d.matchType === 'name'));
  assert.equal(combined.rows.flatMap((r: MealEmployee) => r.cortex).length, 5);
  for (const previous of before.rows) {
    const row = combined.rows.find((r: MealEmployee) => r.id === previous.id);
    assert.deepEqual(row.paycom, previous.paycom);
  }
  names.forEach(([, flex], i) => {
    const row = combined.rows.find((r: MealEmployee) => r.id === `paycom:E00${i + 1}`);
    assert.equal(row.cortex.length, i === 0 ? 2 : 1);
    assert.equal(row.cortex[0].driverName, flex);
    assert.equal(row.cortex[0].lastDelivery, `${date}T11:58:00Z`);
    assert.equal(row.cortex[0].start, `${date}T12:00:00Z`);
    assert.equal(row.cortex[0].end, `${date}T12:30:00Z`);
    assert.equal(row.cortex[0].firstDelivery, `${date}T12:32:00Z`);
  });
  // A decision in Driver Match overrides the automatic rules at once.
  const holder = async (id: string) => {
    let found: Driver | undefined;
    await until(async () => {
      const people: DriverMatch = await owner.read('/api/dsp/driver-match');
      found = people.drivers.find((d) => d.ids.some((i) => i.id === id));
      return Boolean(found);
    });
    return found!;
  };
  const jamie = await holder('driver-0');
  assert.equal(jamie.ids.length, 2);
  const split = { code: jamie.code, source: 'amazon', id: 'driver-0' };
  assert.equal((await owner.post('/api/dsp/driver-match/split', split)).status, 200);
  assert.equal((await owner.get(endpoint)).value.rows.length, before.rows.length + 1);
  const alone = await holder('driver-0');
  const merge = { code: alone.code, into: jamie.code };
  assert.equal((await owner.post('/api/dsp/driver-match/merge', merge)).status, 200);
  assert.equal((await owner.get(endpoint)).value.rows.length, before.rows.length);
});

test('typed workforce responses carry Rust assessments and reject malformed nested records', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const date = f.collector(
    dsp.id,
    (db) => (db.prepare('SELECT max(date) date FROM timecards').get() as { date: string }).date,
  );
  const routes = [
    '/api/dsp/employees',
    '/api/dsp/employees/E001',
    `/api/dsp/timecards?date=${date}`,
    `/api/dsp/paycom/meal-breaks?date=${date}`,
  ];
  const replies = await Promise.all(
    routes.map(async (route) => {
      const reply = await owner.get(route);
      assert.equal(reply.status, 200);
      assert.deepEqual(checkReply([timecardReplies], route, 'GET', reply.value), reply.value);
      return reply.value;
    }),
  );
  const [employees, timecard, daily, comparison] = replies;
  assert(
    timecard.timecards.some(
      (card: { assessment: { events: unknown[] } }) => card.assessment.events.length,
    ),
  );
  assert(comparison.rows.every((row: MealEmployee) => row.assessment.pairs.length > 0));
  for (const [index, corrupt] of [
    [0, (value: any) => (value.employees[0].active = 'yes')],
    [1, (value: any) => delete value.timecards[0].assessment],
    [1, (value: any) => (value.timecards[0].punches[0].hours = 'eight')],
    [2, (value: any) => (value.rows[0].hours = 'eight')],
    [3, (value: any) => (value.rows[0].assessment.status = 'made_up')],
    [3, (value: any) => (value.rows[0].assessment.pairs[0].cortexIndex = 100000)],
  ] as const) {
    const invalid = structuredClone(replies[index]);
    corrupt(invalid);
    assert.throws(
      () => checkReply([timecardReplies], routes[index]!, 'GET', invalid),
      /invalid_api_response/,
    );
  }
  assert.equal(employees.total, 12);
  assert.equal(daily.available, true);
  // Assessment must read persisted DSP preferences and change on the next response.
  const prefs = (await owner.get('/api/dsp/paycom/settings')).value;
  assert.equal(
    (
      await owner.post('/api/dsp/paycom/settings', {
        revision: prefs.revision,
        values: { ...prefs.values, late_da_time: '00:00', late_da_departments: [] },
      })
    ).status,
    200,
  );
  const changed = (await owner.get(routes[3]!)).value;
  assert(changed.rows.some((row: MealEmployee) => row.assessment.lateIn));
});
