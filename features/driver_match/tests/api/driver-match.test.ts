import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import type { DriverMatch, PersonDetails } from '../../api/index.js';

test('Driver Match answers only to its own permission, and its decisions to their shape', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  await member.select(north.id);
  // A member's role holds no Driver Match permission.
  assert.equal((await member.get('/api/dsp/driver-match')).status, 403);
  assert.equal((await member.get('/api/dsp/driver-match/counts')).status, 403);
  await owner.select(north.id);
  // The scheduler gives the demo's people their codes shortly after the server starts.
  let result!: DriverMatch;
  await until(async () => {
    result = await owner.read('/api/dsp/driver-match');
    return Boolean(result.checkedAt) && result.people.length > 1;
  });
  // The badge counts the questions the tab asks, without the roster.
  const counts = await owner.get('/api/dsp/driver-match/counts');
  assert.equal(counts.status, 200, counts.body);
  assert.deepEqual(counts.value, { questions: result.questions.length });
  const [first, second] = result.people;
  const details = await owner.get(`/api/dsp/driver-match/people/${first!.code}`);
  assert.equal(details.status, 200, details.body);
  assert.equal((details.value as PersonDetails).person.code, first!.code);
  // A code written as codes are, which nobody holds.
  assert.equal((await owner.get('/api/dsp/driver-match/people/ZZZZZZ')).status, 404);
  // A decision answers with nothing: the tab reads itself again.
  const apart = await owner.post('/api/dsp/driver-match/apart', {
    code: first!.code,
    other: second!.code,
  });
  assert.deepEqual([apart.status, apart.value], [200, { ok: true }]);
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
});
