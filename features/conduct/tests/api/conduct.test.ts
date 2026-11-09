import test from 'node:test';
import assert from 'node:assert/strict';
import type {
  CategoryInput,
  ConductOverview,
  ConductSaved,
  WriteupInput,
} from '../../api/index.js';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';

const route = '/api/dsp/conduct';

async function setup() {
  const f = await fixture();
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  return { f, owner, north };
}

test('a writeup is written up for someone, suggests the next tier, and locks once signed', async (t) => {
  const { f, owner, north } = await setup();
  t.after(f.close);
  const read = await owner.get(route);
  assert.equal(read.status, 200, read.body);
  const overview: ConductOverview = read.value;
  assert.deepEqual(
    overview.categories.map((c) => c.name),
    [
      'Safety',
      'Excessive Tardies',
      'Unexcused Absences',
      'Late Callout',
      'Breaks',
      'Timecard Punches',
      'Pre-trip & Fuel',
      'Vehicle Damage',
      'POD',
      'CDF',
      'Uniform',
      'OODT',
    ],
  );
  // Each starts at Verbal and steps down a tier at a time, six months at each.
  for (const category of overview.categories) {
    assert.equal(category.startTier, 'verbal', category.name);
    assert.deepEqual(category.stepDowns, [
      { tier: 'final', after: 6, unit: 'months', to: 'written' },
      { tier: 'written', after: 6, unit: 'months', to: 'verbal' },
      { tier: 'verbal', after: 6, unit: 'months', to: null },
    ]);
  }
  const person = overview.people.find((p) => !p.left)!;
  assert(person, 'the demo DSP has someone to write up');
  const tardies = overview.categories.find((c) => c.id === 'tardies')!;
  const input: WriteupInput = {
    person: person.code,
    category: tardies.id,
    tier: 'verbal',
    finalKind: null,
    suspension: null,
    writtenOn: overview.today,
    statement: 'DA was late on three days.',
    prior: tardies.prior,
    plan: tardies.plan,
  };

  const written = await owner.post(`${route}/writeups`, input);
  assert.equal(written.status, 201, written.body);
  const saved: ConductSaved = written.value;
  assert.equal(saved.writeup.personName, person.name);
  assert.equal(
    saved.writeup.supervisorName,
    `${owner.session.user.firstName} ${owner.session.user.lastName}`,
  );
  assert.deepEqual(
    saved.overview.standings.map((s) => [s.person, s.categoryId, s.tier, s.stepsDownTo]),
    [[person.code, 'tardies', 'verbal', null]],
  );
  assert(saved.overview.standings[0]!.stepsDownOn! > overview.today);

  // A suspension is only a final's, and a final with one says how long.
  for (const bad of [
    { ...input, finalKind: 'suspension', suspension: '1 day' },
    { ...input, tier: 'final', finalKind: 'suspension', suspension: null },
    { ...input, person: 'nobody' },
  ]) {
    assert.notEqual((await owner.post(`${route}/writeups`, bad)).status, 201, JSON.stringify(bad));
  }

  const id = saved.writeup.id;
  const signed = await owner.post(`${route}/writeups/${id}/sign`, {
    signedOn: overview.today,
    revision: saved.writeup.revision,
  });
  assert.equal(signed.status, 200, signed.body);
  const after: ConductSaved = signed.value;
  assert.equal(after.writeup.signedOn, overview.today);
  const changed = await owner.post(`${route}/writeups/${id}`, {
    ...input,
    revision: after.writeup.revision,
  });
  assert.equal(changed.status, 409);
  assert.equal(changed.value.error, 'conduct_writeup_signed');

  // Another DSP knows nothing of it.
  const other = owner.session.dsps.find((d: { id: string }) => d.id !== north.id);
  await owner.select(other.id);
  assert.equal((await owner.get(`${route}/writeups/${id}`)).status, 404);
});

test('Conduct answers only those whose role grants it, and categories only those who manage it', async (t) => {
  const { f, north } = await setup();
  t.after(f.close);
  // No default role holds conduct.use, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(north.id);
  assert.equal((await member.get(route)).status, 403);
  assert.equal((await member.post(`${route}/categories/pod/retire`, {})).status, 403);
});

test('a category steps down as its manager sets it, each tier only to a lower one', async (t) => {
  const { f, owner } = await setup();
  t.after(f.close);
  const overview: ConductOverview = (await owner.get(route)).value;
  const pod = overview.categories.find((c) => c.id === 'pod')!;
  const input: CategoryInput = {
    name: pod.name,
    startTier: 'written',
    stepDowns: [
      { tier: 'written', after: 10, unit: 'days', to: null },
      { tier: 'final', after: 2, unit: 'weeks', to: null },
    ],
    statement: pod.statement,
    prior: pod.prior,
    plan: pod.plan,
    revision: pod.revision,
  };
  for (const stepDowns of [
    [{ tier: 'written', after: 1, unit: 'days', to: 'final' }],
    [{ tier: 'final', after: 0, unit: 'days', to: null }],
    [
      { tier: 'final', after: 1, unit: 'days', to: null },
      { tier: 'final', after: 2, unit: 'days', to: 'verbal' },
    ],
  ]) {
    const refused = await owner.post(`${route}/categories/pod`, { ...input, stepDowns });
    assert.equal(refused.status, 400, JSON.stringify(stepDowns));
  }
  const changed = await owner.post(`${route}/categories/pod`, input);
  assert.equal(changed.status, 200, changed.body);
  const after: ConductOverview = changed.value;
  const kept = after.categories.find((c) => c.id === 'pod')!;
  assert.equal(kept.startTier, 'written');
  // Highest tier first; Verbal, left out, never steps down.
  assert.deepEqual(kept.stepDowns, [
    { tier: 'final', after: 2, unit: 'weeks', to: null },
    { tier: 'written', after: 10, unit: 'days', to: null },
  ]);
});
