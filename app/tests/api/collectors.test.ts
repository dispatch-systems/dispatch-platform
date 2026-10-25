import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';
import '../support/manifests.js';
import { collections } from '../../../core/collection/api/generated/collections.js';
import { collectedOf } from '../../../core/shell/frontend/runtime/slots.js';

// Settings' Collectors page, over HTTP: core's routes, with the collections Timecard keeps from
// Paycom. Who sets a collection's schedule is its keeper's to say; its rules are saved together
// at the revision they were read at; and each DSP's are its own. What each collection collects
// is the feature's that keeps it, as the page shows it.

const everyDay = [0, 1, 2, 3, 4, 5, 6];
const at = (times: string[], collects = 'paycom') => ({
  id: null,
  collects,
  kind: 'times',
  days: everyDay,
  times,
});
type Collection = {
  collection: string;
  label: string;
  enabled: boolean;
  revision: string;
  rules: any[];
};

test("a collection's schedule is set by those its keeper names, at the revision it was read at, in its DSP alone", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const collection = async (client: typeof owner, name: string): Promise<Collection> =>
    (await client.read('/api/dsp/collectors')).collections.find(
      (each: Collection) => each.collection === name,
    );
  const save = (client: typeof owner, name: string, rules: unknown[], revision: string) =>
    client.post(`/api/dsp/collectors/${name}/schedule`, { rules, revision });

  // Nothing is scheduled until a member schedules it, in the DSP's own timezone.
  const listed = (await owner.get('/api/dsp/collectors')).value;
  assert.equal(listed.timezone, north.timezone);
  assert(listed.collections.every((each: Collection) => !each.rules.length && !each.enabled));
  const empty = await collection(owner, 'paycom');

  // A rule is refused unless its days and times hold and it reads what its collection offers.
  for (const [rule, code] of [
    [{ ...at(['06:00']), days: [] }, 'invalid_schedule_days'],
    [at(['25:00']), 'invalid_schedule_time'],
    [
      { ...at([]), kind: 'repeat', every: 45, from: '06:00', to: '18:00' },
      'invalid_schedule_interval',
    ],
    [at(['06:00'], 'routes'), 'invalid_input'],
    [{ ...at(['06:00']), kind: 'daily' }, 'invalid_input'],
  ] as const) {
    const refused = await save(owner, 'paycom', [rule], empty.revision);
    assert.equal(refused.value.error, code, JSON.stringify(rule));
  }

  const created = await save(owner, 'paycom', [at(['06:00'])], empty.revision);
  assert.equal(created.status, 200, created.body);
  assert.equal(created.value.enabled, true);
  const [rule] = created.value.rules;
  assert(rule.id && rule.nextRun);
  // Saved at a revision another save passed, or naming a rule it doesn't have, it's stale.
  for (const [rules, revision] of [
    [[at(['07:00'])], empty.revision],
    [[{ ...at(['07:00']), id: 'schedule_elsewhere' }], created.value.revision],
  ] as const) {
    const stale = await save(owner, 'paycom', [...rules], revision);
    assert.deepEqual([stale.status, stale.value.error], [409, 'schedule_changed']);
  }
  // An edit keeps its rule; a rule left out goes.
  const edited = await save(
    owner,
    'paycom',
    [
      { ...at(['14:00', '07:00']), id: rule.id },
      { ...at([]), kind: 'repeat', every: 30, from: '00:00', to: '00:00' },
    ],
    created.value.revision,
  );
  assert.equal(edited.status, 200, edited.body);
  assert.equal(edited.value.rules[0].id, rule.id);
  assert.deepEqual(edited.value.rules[0].times, ['07:00', '14:00']);
  // A preview runs what it's sent, unsaved: all day every 30 minutes.
  const preview = await owner.post('/api/dsp/collectors/paycom/preview', {
    rules: [edited.value.rules[1]].map(({ nextRun: _, lastError: __, ...each }) => each),
  });
  assert.equal(preview.status, 200, preview.body);
  const runs = preview.value.runs.map((run: string) => Date.parse(run));
  assert.equal(runs.length, 5);
  for (let index = 1; index < runs.length; index++)
    assert.equal(runs[index] - runs[index - 1], 30 * 60_000);

  // Off, every rule waits, and stays off across a restart; on, each runs again.
  const kept = edited.value.rules.map(({ nextRun: _, lastError: __, ...each }: any) => each);
  const off = async (enabled: boolean, revision: string) =>
    owner.post('/api/dsp/collectors/paycom/enabled', { enabled, revision });
  assert.equal((await off(false, created.value.revision)).value.error, 'schedule_changed');
  const paused = await off(false, edited.value.revision);
  assert.equal(paused.status, 200, paused.body);
  assert(paused.value.rules.every((each: any) => each.nextRun === null));
  await f.stop();
  await f.start();
  assert.equal((await collection(owner, 'paycom')).enabled, false);
  const resumed = await off(true, paused.value.revision);
  assert(resumed.value.enabled && resumed.value.rules.every((each: any) => each.nextRun));

  // Each change is in the DSP's log, by the collection it scheduled.
  const events = (await owner.read('/api/platform/audit?limit=50')).events;
  for (const [action, field] of [
    ['schedule.updated', 'rules'],
    ['schedule.toggled', 'enabled'],
  ])
    assert(
      events.some(
        (e: any) =>
          e.action === action &&
          e.detail === empty.label &&
          e.changes.some((change: any) => change.field === field),
      ),
      action,
    );

  // A member sets only the schedules their role lets them: none without a permission that
  // sets one, and with Timecard's, Timecard's collections alone.
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/collectors')).status, 403);
  const role = (await owner.read('/api/dsp/roles')).find((r: any) => r.name === 'Member');
  const grant = async (permissions: string[]) => {
    const changed = await owner.post(`/api/dsp/roles/${role.id}`, { name: 'Member', permissions });
    assert.equal(changed.status, 200, changed.body);
    await member.select(north.id);
  };
  await grant(['timecard.manage']);
  const theirs = (await member.read('/api/dsp/collectors')).collections;
  assert.deepEqual(
    theirs.map((each: Collection) => each.collection),
    ['paycom', 'meal_break'],
  );
  assert(theirs.every((each: { canCollect: boolean }) => !each.canCollect));
  // A role's change expires every open view of the DSP, the owner's too.
  await owner.select(north.id);
  const dvic = await collection(owner, 'dvic');
  const denied = await save(member, 'dvic', [at(['06:00'], 'dvic')], dvic.revision);
  assert.deepEqual([denied.status, denied.value.error], [403, 'permission_denied']);
  const now = { collects: 'paycom', requestId: 'collect-now' };
  assert.equal((await member.post('/api/dsp/collectors/paycom/collect', now)).status, 403);
  await grant(['timecard.manage', 'collections.run']);
  const collected = await member.post('/api/dsp/collectors/paycom/collect', now);
  assert.equal(collected.status, 202, collected.body);
  assert.deepEqual(
    collected.value.map((job: { kind: string }) => job.kind),
    ['paycom.collect'],
  );
  // Pressed twice, it's queued once.
  const again = await member.post('/api/dsp/collectors/paycom/collect', now);
  assert.equal(again.value[0].id, collected.value[0].id);

  // Another DSP's schedule is its own: none of Northline's rules there, nor reached from there.
  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(summit.id);
  const before = await collection(owner, 'paycom');
  assert.deepEqual(before.rules, []);
  // Its Paycom isn't connected: its first rule waits, off.
  const own = await save(owner, 'paycom', [at(['05:00'])], before.revision);
  assert.equal(own.status, 200, own.body);
  assert.equal(own.value.enabled, false);
  const reached = await save(owner, 'paycom', kept.slice(0, 1), own.value.revision);
  assert.deepEqual([reached.status, reached.value.error], [409, 'schedule_changed']);
  assert.deepEqual(
    (await collection(owner, 'paycom')).rules.map((each) => each.times),
    [['05:00']],
  );
  await owner.select(north.id);
  assert.deepEqual(
    (await collection(owner, 'paycom')).rules.map((each) => each.id),
    kept.map((each: { id: string }) => each.id),
  );
});

test('what every collection collects is said by the feature that keeps it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  await owner.select(
    owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics').id,
  );
  const shown: { collection: string; feature: string }[] = (await owner.read('/api/dsp/collectors'))
    .collections;
  for (const { schedule, label } of collections) {
    const each = shown.find((collection) => collection.collection === schedule);
    assert(each, `${label} isn't on the Collectors page`);
    assert(
      await collectedOf(each.feature, schedule),
      `${each.feature} doesn't say what ${label} collects`,
    );
  }
});
