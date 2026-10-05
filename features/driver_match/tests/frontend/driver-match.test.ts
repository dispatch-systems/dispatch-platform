import test from 'node:test';
import assert from 'node:assert/strict';
import type { Driver, DriverId } from '../../api/index.js';
import {
  dataAmount,
  dayLabel,
  driverMatches,
  evidenceSupports,
  evidenceText,
  goesBy,
  inFilter,
  initials,
  seenRange,
  shortId,
} from '../../frontend/driver-match.js';

const id = (source: DriverId['source'], value: string, name: string): DriverId => ({
  source,
  id: value,
  name,
  department: null,
  position: null,
  firstSeen: null,
  lastSeen: null,
  linkedBy: 'name',
  linkedAt: '2026-09-30T00:00:00Z',
});
const antonio: Driver = {
  code: 'P3ZT9Q',
  name: 'Antonio Reyes',
  status: 'confirmed',
  ids: [id('paycom', 'R4KD', 'REYES, ANTONIO'), id('amazon', 'A2F7Q9XK3LM8PZ', 'Tony Reyes')],
  appears: ['timecards', 'routes'],
  lastSeen: '2026-09-29',
};

test('evidence reads as short sentences, and only days that disagree count against a pair', () => {
  const evidence = (kind: string, extra = {}) =>
    ({ kind, short: null, long: null, same: null, of: null, ...extra }) as never;
  assert.equal(
    evidenceText(evidence('short_form', { short: 'Tony', long: 'Antonio' })),
    'Tony is short for Antonio',
  );
  assert.equal(
    evidenceText(evidence('worked_days', { same: 11, of: 11 })),
    'Clocked in on all 11 days they drove',
  );
  const partly = evidence('worked_days', { same: 4, of: 5 });
  assert.equal(evidenceText(partly), 'Clocked in on 4 of 5 days they drove');
  assert.equal(evidenceSupports(partly), false);
  assert.equal(evidenceSupports(evidence('same_last_name')), true);
});

test('days read as Today, Yesterday or a date, with the year only when it differs', () => {
  assert.equal(dayLabel('2026-09-30', '2026-09-30'), 'Today');
  assert.equal(dayLabel('2026-09-29', '2026-09-30'), 'Yesterday');
  assert.equal(dayLabel('2026-09-02', '2026-09-30'), 'Sep 2');
  assert.equal(dayLabel('2025-12-31', '2026-01-02'), 'Dec 31, 2025');
  assert.equal(dayLabel(null, '2026-09-30'), '—');
  assert.equal(seenRange('2026-09-30', '2026-09-30', '2026-09-30'), 'Seen Today');
  assert.equal(seenRange('2026-09-02', '2026-09-29', '2026-09-30'), 'Seen Sep 2 – Yesterday');
  assert.equal(seenRange(null, null, '2026-09-30'), 'Not seen in collected data');
});

test('people are found by any name, code or ID, and filtered by where they stand', () => {
  for (const query of ['tony', 'REYES', 'p3zt', 'r4kd', 'A2F7Q9'])
    assert.ok(driverMatches(antonio, query), query);
  assert.ok(!driverMatches(antonio, 'jordan'));
  assert.ok(inFilter(antonio, 'matched') && inFilter(antonio, 'all'));
  assert.ok(!inFilter(antonio, 'review'));
  const gone: Driver = { ...antonio, status: 'former', ids: [antonio.ids[1]!] };
  assert.ok(inFilter(gone, 'former') && !inFilter(gone, 'amazon'));
  assert.equal(goesBy(antonio), 'Tony');
  assert.equal(goesBy({ ...antonio, ids: [antonio.ids[0]!] }), undefined);
});

test('IDs, initials and amounts are short and plain', () => {
  assert.equal(shortId(antonio.ids[1]!), 'A2F7…M8PZ');
  assert.equal(shortId(antonio.ids[0]!), 'R4KD');
  assert.equal(initials('Ana Sofía Morales'), 'AS');
  assert.equal(dataAmount('dvic', 1), '1 inspection');
  assert.equal(dataAmount('weekly_scorecard', 28), '28 weeks');
  assert.equal(dataAmount('routes', 131), '131 days');
});
