import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';

// The schedules of the collections Timecard keeps, as Settings' Collectors page sets them: what
// each needs before it runs, and the scheduler running one.

const daily = (collects: string, time = '06:00') => ({
  id: null,
  collects,
  kind: 'times',
  days: [0, 1, 2, 3, 4, 5, 6],
  times: [time],
});
const every = (collects: string) => ({
  id: null,
  collects,
  kind: 'repeat',
  days: [0, 1, 2, 3, 4, 5, 6],
  every: 120,
  from: '00:00',
  to: '00:00',
});
type Collection = { collection: string; enabled: boolean; revision: string; rules: any[] };

test('meal break schedules wait for Cortex and a station, and a disconnect pauses only what needs it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const collection = async (name: string): Promise<Collection> =>
    (await owner.read('/api/dsp/collectors')).collections.find(
      (each: Collection) => each.collection === name,
    );
  const save = async (name: string, rule: unknown) =>
    owner.post(`/api/dsp/collectors/${name}/schedule`, {
      rules: [rule],
      revision: (await collection(name)).revision,
    });
  const on = async (name: string) =>
    owner.post(`/api/dsp/collectors/${name}/enabled`, {
      enabled: true,
      revision: (await collection(name)).revision,
    });
  // Its first rule is kept, off, until what it reads is ready.
  const waiting = await save('meal_break', daily('meal_break'));
  assert.equal(waiting.status, 200, waiting.body);
  assert.equal(waiting.value.enabled, false);
  assert.equal((await on('meal_break')).value.error, 'schedule_meals_required');
  await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal((await on('meal_break')).value.error, 'schedule_scope_required');
  const seed = await owner.post('/api/dsp/cortex/meal-breaks/collect', {
    requestId: 'schedule-scope',
    date: '2026-01-10',
    station: 'DEMO1',
    serviceAreaId: 'area-demo',
    provider: 'provider-demo',
    timezone: dsp.timezone,
  });
  assert.equal(seed.status, 202, seed.body);
  await until(
    async () =>
      (await owner.read('/api/dsp/jobs')).find((j: any) => j.id === seed.value.id)?.status ===
      'succeeded',
  );
  const meals = await on('meal_break');
  assert.equal(meals.status, 200, meals.body);
  assert.equal(meals.value.enabled, true);
  assert.equal((await save('paycom', daily('paycom'))).value.enabled, true);
  assert.equal(
    (await owner.post('/api/dsp/connections/cortex/disable', { removeCredentials: false })).status,
    200,
  );
  assert.equal((await collection('meal_break')).enabled, false);
  assert.equal((await collection('paycom')).enabled, true);
  // Nor does it come on again until Cortex is connected.
  assert.equal((await on('meal_break')).value.error, 'schedule_meals_required');
});

test('the live scheduler collects an overdue occurrence once and advances to the next run', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: any) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const paycom = (await owner.read('/api/dsp/collectors')).collections.find(
    (each: Collection) => each.collection === 'paycom',
  );
  const result = await owner.post('/api/dsp/collectors/paycom/schedule', {
    rules: [every('paycom')],
    revision: paycom.revision,
  });
  assert.equal(result.status, 200, result.body);
  const key = result.value.rules[0].id;
  await f.stop();
  f.database(`dsps/${dsp.id}/data/dispatch.sqlite`, (db) =>
    db
      .prepare('UPDATE collection_schedules SET next_run=? WHERE id=?')
      .run('2026-01-01T00:00:00.000Z', key),
  );
  await f.start();
  await until(async () =>
    (await owner.read('/api/dsp/jobs')).some(
      (j: any) => j.actorId === null && j.status === 'succeeded',
    ),
  );
  const rule = (await owner.read('/api/dsp/collectors')).collections
    .find((each: Collection) => each.collection === 'paycom')
    .rules.find((each: any) => each.id === key);
  assert(new Date(rule.nextRun).getTime() > Date.now());
  const count = (await owner.get('/api/dsp/jobs')).value.length;
  await f.stop();
  await f.start();
  assert.equal((await owner.get('/api/dsp/jobs')).value.length, count);
});
