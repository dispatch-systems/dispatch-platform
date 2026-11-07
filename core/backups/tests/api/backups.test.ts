import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture } from '../../../shell/tests/support/support.js';

// A DSP's Backups tab over HTTP: only the platform owner reads or changes it; its rules save only
// over the schedule they were read from; and without somewhere to keep backups, none is taken.
// Taking, keeping and letting go of backups is tested beside them, against a stand-in bucket.
test('only the platform owner schedules a DSP’s backups, and none is taken without a bucket', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps[0];
  const backups = `/api/platform/dsps/${north.id}/backups`;
  assert.equal((await member.get(backups)).status, 403);
  assert.equal((await member.post(`${backups}/now`)).status, 403);

  // Unsaved, it has the rule every DSP starts with, holding all but its pictures.
  const first = await owner.read(backups);
  assert.equal(first.configured, false);
  assert.deepEqual(
    first.rules.map(
      (rule: { times: string[]; keep: number; keepUnit: string; skipped: string[] }) => [
        rule.times,
        rule.keep,
        rule.keepUnit,
        rule.skipped,
      ],
    ),
    [[['02:00'], 30, 'day', ['pictures']]],
  );
  assert.deepEqual(
    first.items
      .filter((item: { always: boolean }) => item.always)
      .map((item: { id: string }) => item.id),
    ['dispatch'],
  );

  const rule = {
    id: null,
    kind: 'repeat',
    days: [0, 1, 2, 3, 4],
    times: [],
    every: 240,
    from: '06:00',
    to: '22:00',
    keep: 2,
    keepUnit: 'week',
    skipped: [],
  };
  const saved = await owner.post(`${backups}/schedule`, {
    revision: first.revision,
    rules: [rule],
  });
  assert.equal(saved.status, 200);
  assert.deepEqual(
    [saved.value.rules.length, saved.value.rules[0].every, saved.value.rules[0].keepUnit],
    [1, 240, 'week'],
  );
  const stale = await owner.post(`${backups}/schedule`, { revision: first.revision, rules: [] });
  assert.deepEqual([stale.status, stale.value.error], [409, 'backup_schedule_changed']);
  const refused = await owner.post(`${backups}/now`);
  assert.deepEqual([refused.status, refused.value.error], [409, 'backups_not_configured']);
});
