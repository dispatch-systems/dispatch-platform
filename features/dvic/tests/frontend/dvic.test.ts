import assert from 'node:assert/strict';
import test from 'node:test';
import {
  driversByCount,
  filterInspections,
  groupByVehicleClass,
  inspectionBand,
  inspectionClock,
  inspectionDuration,
  inspectionDate,
  inspectionWeekday,
  inspectionShortfall,
  readInspectionWeek,
  repeatDrivers,
  shortestFirst,
  shortestByDay,
  weekDays,
  weekStart,
} from '../../dashboard/src/lib/dvic.js';
import type { DvicInspection } from '../../shared/contracts/dvic.js';
import { mutationAffects } from '../../dashboard/src/lib/data-policy.js';

const row = (id: string, fleetType = 'CV'): DvicInspection => ({
  id,
  startDate: '2026-09-26',
  driverId: 'driver-1',
  driverName: 'Taylor Brooks',
  vin: '1FIXTURE000000001',
  fleetType,
  inspectionType: 'PRE_TRIP_DVIC',
  status: 'PASSED',
  startTime: '2026-09-26 23:59:01',
  endTime: '2026-09-27 00:00:15',
  durationSeconds: 74,
  minimumSeconds: 90,
  shortBySeconds: 16,
  sourceReportDate: '2026-09-27',
});

test('inspection weeks use Sunday dates across year and DST boundaries; source clocks never shift', () => {
  assert.equal(weekStart('2026-09-26'), '2026-09-20');
  assert.equal(weekStart('2027-01-01'), '2026-12-27');
  assert.equal(weekDays('2026-03-08').at(-1), '2026-03-14');
  assert.equal(inspectionDate('2026-09-26'), 'Sep 26');
  assert.equal(inspectionDate('2026-09-26', true), 'Sat, Sep 26');
  assert.equal(inspectionWeekday('2026-09-26'), 'Sat');
  assert.equal(inspectionClock('2026-09-26 23:59:01'), '11:59:01 PM');
  assert.equal(inspectionClock('2026-09-27T00:00:15'), '12:00:15 AM');
  assert.equal(inspectionDuration(89.9999), '1m 29s');
  assert.equal(inspectionDuration(69.7), '1m 9s');
  assert.equal(inspectionDuration(0.0001), '<1s');
  assert.equal(inspectionDuration(0), '0s');
  assert.equal(inspectionDuration(300), '5m');
  assert.equal(inspectionShortfall(0.0001), '1s');
  assert.equal(inspectionShortfall(90 - 69.7), '21s');
  assert.equal(inspectionShortfall(56), '56s');
  assert.equal(inspectionShortfall(20.000000000000004), '20s');
  assert.equal(inspectionDuration(19.999999999999996), '20s');
  assert.equal(inspectionShortfall(0), '0s');
});
test('filters use driver identity, name or VIN and preserve distinct inspections', () => {
  const rows = [row('a'), row('b', 'SV')];
  assert.equal(filterInspections(rows, '  BROOKS ', '').length, 2);
  assert.equal(filterInspections(rows, 'brooks', 'dot')[0]?.id, 'b');
  assert.equal(filterInspections(rows, 'driver-1', '').length, 2);
  assert.equal(filterInspections(rows, 'fixture', '').length, 0, 'VINs are not searched');
  assert.deepEqual(
    filterInspections([...rows, row('c', 'CDV')], '', 'non-dot').map((item) => item.id),
    ['a', 'c'],
  );
});
test('complete weeks follow cursors, deduplicate identities and fail rather than publishing partial totals', async () => {
  const calls: (string | null)[] = [];
  const signal = new AbortController().signal;
  const rows = await readInspectionWeek(async (after) => {
    calls.push(after);
    return after
      ? { inspections: [row('a'), row('b')], nextCursor: null }
      : { inspections: [row('a')], nextCursor: 'next' };
  }, signal);
  assert.deepEqual(calls, [null, 'next']);
  assert.deepEqual(
    rows.map((item) => item.id),
    ['a', 'b'],
  );
  await assert.rejects(
    readInspectionWeek(async () => ({ inspections: [row('a')], nextCursor: 'loop' }), signal),
    /changed while loading/,
  );
  await assert.rejects(
    readInspectionWeek(async (after) => {
      if (after) throw new Error('offline');
      return { inspections: [row('a')], nextCursor: 'next' };
    }, signal),
    /offline/,
  );
  const abort = new AbortController();
  await assert.rejects(
    readInspectionWeek(async () => {
      abort.abort();
      return { inspections: [row('a')], nextCursor: null };
    }, abort.signal),
    { name: 'AbortError' },
  );
});
test('DVIC mutations refresh DVIC without invalidating timecard data', () => {
  assert.equal(mutationAffects('/api/dsp/dvic/collect', '/api/dsp/dvic/status'), true);
  assert.equal(
    mutationAffects('/api/dsp/dvic/schedules/a/enabled', '/api/dsp/dvic/schedules'),
    true,
  );
  assert.equal(mutationAffects('/api/dsp/dvic/collect', '/api/dsp/timecards'), false);
});

test('bands follow the share of the minimum with exact boundaries and a capped share', () => {
  const at = (durationSeconds: number, minimumSeconds: 90 | 300 = 90) => ({
    ...row('x'),
    durationSeconds,
    minimumSeconds,
    shortBySeconds: minimumSeconds - durationSeconds,
  });
  assert.equal(inspectionBand(at(67.5)), 'high');
  assert.equal(inspectionBand(at(67.499)), 'mid');
  assert.equal(inspectionBand(at(31.5)), 'mid');
  assert.equal(inspectionBand(at(31.499)), 'low');
  assert.equal(inspectionBand(at(0)), 'low');
  assert.equal(inspectionBand(at(225, 300)), 'high');
  assert.equal(inspectionBand(at(104, 300)), 'low');
  assert.deepEqual(
    shortestFirst([at(80), at(20, 300), at(20)]).map((item) => item.durationSeconds),
    [20, 20, 80],
  );
});
test('repeat drivers and driver grouping follow identity, not the displayed name', () => {
  const week: DvicInspection[] = [
    { ...row('a'), durationSeconds: 20, shortBySeconds: 70 },
    { ...row('b'), startDate: '2026-09-25', durationSeconds: 10, shortBySeconds: 80 },
    { ...row('c'), startDate: '2026-09-24', durationSeconds: 80, shortBySeconds: 10 },
    {
      ...row('d'),
      driverId: 'driver-2',
      driverName: 'Jordan Lee',
      durationSeconds: 89.4,
      shortBySeconds: 0.6,
    },
    { ...row('e'), driverId: 'driver-3', driverName: 'Old Name', startDate: '2026-09-22' },
    { ...row('f'), driverId: 'driver-3', driverName: 'New Name', startDate: '2026-09-23' },
  ];
  const day = week.filter((item) => item.startDate === '2026-09-26');
  assert.deepEqual(repeatDrivers(week, day), ['Taylor Brooks']);
  assert.deepEqual(repeatDrivers(week, day, 1), ['Taylor Brooks', 'Jordan Lee']);
  const drivers = driversByCount(week);
  assert.deepEqual(
    drivers.map((driver) => [driver.driverId, driver.driverName, driver.rows.length]),
    [
      ['driver-1', 'Taylor Brooks', 3],
      ['driver-3', 'New Name', 2],
      ['driver-2', 'Jordan Lee', 1],
    ],
  );
  const step = { ...row('s', 'SV'), minimumSeconds: 300 as const, shortBySeconds: 226 };
  assert.deepEqual(
    groupByVehicleClass([step, row('v'), row('c', 'CDV')]).map((group) => [
      group.vehicles,
      group.minimum,
      group.rows.map((item) => item.id),
    ]),
    [
      ['non-dot', 90, ['c', 'v']],
      ['dot', 300, ['s']],
    ],
  );
  assert.deepEqual(
    groupByVehicleClass([row('v')]).map((group) => group.vehicles),
    ['non-dot'],
  );
});

test('day cells keep the same minimum share and deterministic ties as the full inspection ordering', () => {
  const rows = [
    { ...row('z'), startDate: '2026-09-25', durationSeconds: 20 },
    { ...row('b'), durationSeconds: 30 },
    { ...row('a'), durationSeconds: 30 },
    { ...row('step', 'SV'), minimumSeconds: 300 as const, durationSeconds: 100 },
    { ...row('early'), durationSeconds: 30, startTime: '2026-09-26 08:00:00' },
  ];
  const before = rows.map((item) => item.id);
  const cells = shortestByDay(rows);
  for (const day of ['2026-09-25', '2026-09-26'])
    assert.equal(cells.get(day), shortestFirst(rows.filter((item) => item.startDate === day))[0]);
  assert.equal(cells.get('2026-09-26')?.id, 'early');
  assert.deepEqual(
    rows.map((item) => item.id),
    before,
    'Grouping must not reorder source rows',
  );
  assert.equal(shortestByDay([]).size, 0);
});
