import test from 'node:test';
import assert from 'node:assert/strict';
import type { Driver, DriverMatch } from '../../../features/driver_match/api/index.js';
import type { MealEmployee } from '../../../features/timecard/api/index.js';
import {
  seedNameVariants,
  variants,
} from '../../../features/timecard/tests/support/name-variants.js';
import { fixture, until } from '../../../core/shell/tests/support/support.js';

test("a decision in Driver Match changes Timecard's meal breaks at once", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const { endpoint, before } = await seedNameVariants(f, owner, dsp.id);
  const rows = async () => (await owner.read(endpoint)).rows.length;
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
  assert.equal(await rows(), before.rows.length + 1);
  const alone = await holder('driver-0');
  const merge = { code: alone.code, into: jamie.code };
  assert.equal((await owner.post('/api/dsp/driver-match/merge', merge)).status, 200);
  assert.equal(await rows(), before.rows.length);
});

test("drivers Driver Match joins share their employee's meal-break row, punches and meals kept", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const { date, endpoint, before } = await seedNameVariants(f, owner, dsp.id);
  // Driver Match joins each driver to their employee shortly after the server starts.
  let combined: { rows: MealEmployee[]; drivers: { matchType: string }[] };
  await until(async () => {
    combined = await owner.read(endpoint);
    return combined.drivers.every((d) => d.matchType === 'name');
  });
  assert.equal(combined!.rows.length, before.rows.length);
  assert.equal(combined!.drivers.length, 4);
  assert.equal(combined!.rows.flatMap((r) => r.cortex).length, 5);
  for (const previous of before.rows) {
    const row = combined!.rows.find((r) => r.id === previous.id)!;
    assert.deepEqual(row.paycom, previous.paycom);
  }
  variants.forEach(([, flex], i) => {
    const row = combined!.rows.find((r) => r.id === `paycom:E00${i + 1}`)!;
    assert.equal(row.cortex.length, i === 0 ? 2 : 1);
    assert.equal(row.cortex[0]!.driverName, flex);
    assert.equal(row.cortex[0]!.lastDelivery, `${date}T11:58:00Z`);
    assert.equal(row.cortex[0]!.start, `${date}T12:00:00Z`);
    assert.equal(row.cortex[0]!.end, `${date}T12:30:00Z`);
    assert.equal(row.cortex[0]!.firstDelivery, `${date}T12:32:00Z`);
  });
});
