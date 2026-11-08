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

  // Unsaved, it has the rule every DSP starts with, kept 30 days, holding all but its pictures.
  const first = await owner.read(backups);
  assert.equal(first.configured, false);
  const [starting] = first.rules;
  assert.deepEqual(
    [first.rules.length, starting.on, starting.times, starting.keep, starting.keepCount],
    [1, 'week', ['02:00'], 'for', 30],
  );
  assert.deepEqual(starting.skipped, ['pictures']);
  assert.deepEqual(
    first.items
      .filter((item: { always: boolean }) => item.always)
      .map((item: { id: string }) => item.id),
    ['dispatch'],
  );

  // The 1st, the 15th and the last day of each month, kept as its last 12.
  const rule = {
    id: null,
    on: 'month',
    days: [],
    dates: [1, 15],
    lastDay: true,
    kind: 'times',
    times: ['02:00'],
    every: null,
    from: null,
    to: null,
    keep: 'last',
    keepCount: 12,
    keepUnit: null,
    skipped: [],
  };
  const saved = await owner.post(`${backups}/schedule`, {
    revision: first.revision,
    rules: [rule],
  });
  assert.equal(saved.status, 200);
  const [kept] = saved.value.rules;
  assert.deepEqual(
    [saved.value.rules.length, kept.dates, kept.lastDay, kept.keep, kept.keepCount],
    [1, [1, 15], true, 'last', 12],
  );
  const stale = await owner.post(`${backups}/schedule`, { revision: first.revision, rules: [] });
  assert.deepEqual([stale.status, stale.value.error], [409, 'backup_schedule_changed']);
  // The audit log keeps the rules as they were and became; saved again unchanged, nothing.
  const again = await owner.post(`${backups}/schedule`, {
    revision: saved.value.revision,
    rules: saved.value.rules,
  });
  assert.equal(again.status, 200, again.body);
  type Logged = { action: string; changes: { field: string; from: string; to: string }[] };
  const logged: Logged[] = (await owner.read('/api/platform/audit')).events.filter(
    (event: Logged) => event.action === 'dsp.backup_schedule_changed',
  );
  assert.equal(logged.length, 1);
  const [{ field, from, to }] = logged[0]!.changes;
  const rules = (value: string) =>
    (JSON.parse(value) as { on: string; dates: number[]; keep: string }[]).map((each) => [
      each.on,
      each.dates,
      each.keep,
    ]);
  assert.deepEqual(
    [field, rules(from), rules(to)],
    ['backupRules', [['week', [], 'for']], [['month', [1, 15], 'last']]],
  );
  const refused = await owner.post(`${backups}/now`);
  assert.deepEqual([refused.status, refused.value.error], [409, 'backups_not_configured']);
});
