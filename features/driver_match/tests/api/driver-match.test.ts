import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../core/shell/tests/support/support.js';
import type { DriverMatch, PersonDetails } from '../../api/index.js';
import { seedDriverMatch } from '../support/driver-match.js';

test('Driver Match answers only to its own permission, and its decisions to their shape', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const north = await seedDriverMatch(f);
  const member = await f.client('member@dispatch.test');
  await member.select(north.id);
  // A member's role holds no Driver Match permission.
  assert.equal((await member.get('/api/dsp/driver-match')).status, 403);
  assert.equal((await member.get('/api/dsp/driver-match/counts')).status, 403);
  // The owner in two tabs, one on each DSP: opening a DSP clears every kept read, so both are
  // open before anything is read.
  const owner = await f.client();
  const other = await f.client();
  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(north.id);
  await other.select(summit.id);
  const result: DriverMatch = await owner.read('/api/dsp/driver-match');
  // The sidebar counts the questions the page asks, without the roster. The count kept for
  // Northline's managers is Northline's alone: Summit, whose data names nobody, has none.
  const counts = '/api/dsp/driver-match/counts';
  assert.deepEqual(await owner.read(counts), { questions: result.questions.length });
  assert.deepEqual(await other.read(counts), { questions: 0 });
  const [first, second] = result.people;
  const details = await owner.get(`/api/dsp/driver-match/people/${first!.code}`);
  assert.equal(details.status, 200, details.body);
  assert.equal((details.value as PersonDetails).person.code, first!.code);
  // A code written as codes are, which nobody holds.
  assert.equal((await owner.get('/api/dsp/driver-match/people/ZZZZZZ')).status, 404);
  // A decision answers with nothing: the page reads itself again, and the count with it.
  const pair = result.questions.find((question) => question.kind === 'pair')!;
  const apart = await owner.post('/api/dsp/driver-match/apart', {
    code: pair.person,
    other: pair.candidate,
  });
  assert.deepEqual([apart.status, apart.value], [200, { ok: true }]);
  assert.deepEqual(await owner.read(counts), { questions: result.questions.length - 1 });
  for (const body of [
    { code: first!.code, into: first!.code },
    { code: 'short', into: second!.code },
    { code: first!.code, into: second!.code, extra: true },
  ])
    assert.equal((await owner.post('/api/dsp/driver-match/merge', body)).status, 400);
  assert.equal(
    (await owner.post('/api/dsp/driver-match/split', { code: first!.code, source: 'x', id: 'y' }))
      .status,
    400,
  );
  // A title is one of the list, and every feature asking the directory sees it at once.
  const here = result.people.find((person) => person.role !== 'former')!;
  const directory = `/api/dsp/people/${here.code}`;
  assert.equal((await owner.read(directory)).title, null);
  const title = (body: object) => owner.post('/api/dsp/driver-match/title', body);
  for (const body of [
    { code: here.code, title: 'boss' },
    { code: 'short', title: 'owner' },
    { code: here.code, title: 'owner', extra: true },
  ])
    assert.equal((await title(body)).status, 400);
  assert.deepEqual((await title({ code: here.code, title: 'dispatch' })).value, { ok: true });
  const titled = await owner.read(directory);
  assert.deepEqual([titled.role, titled.title], ['office', 'dispatch']);
  await title({ code: here.code, title: null });
  assert.equal((await owner.read(directory)).title, null);
});
