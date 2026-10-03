import test from 'node:test';
import assert from 'node:assert/strict';
import {
  collectionAffects,
  mutationAffects,
  type CollectionChange,
} from '../../frontend/lib/data-policy.js';

test('a driver checkpoint touches its employee and days, not another driver, roster, or feature', () => {
  const changes: CollectionChange[] = [
    { provider: 'paycom', dates: ['2026-09-22'], employeeCode: 'A', roster: false },
  ];
  for (const url of [
    '/api/dsp/employees/A?from=2026-09-20',
    '/api/dsp/timecards?date=2026-09-22',
    '/api/dsp/paycom/meal-breaks?date=2026-09-22',
    '/api/dsp/jobs/meal-breaks?date=2026-09-22',
  ])
    assert.equal(collectionAffects(url, changes), true, url);
  for (const url of [
    '/api/dsp/employees/B',
    '/api/dsp/employees?limit=100',
    '/api/dsp/timecards?date=2026-09-21',
    '/api/dsp/members',
    '/api/platform/dsps',
  ])
    assert.equal(collectionAffects(url, changes), false, url);
  assert.equal(
    collectionAffects('/api/dsp/employees?limit=100', [{ ...changes[0]!, roster: true }]),
    true,
  );
  assert.equal(
    collectionAffects('/api/dsp/timecards?date=2026-09-22', [
      { ...changes[0]!, provider: 'cortex' },
    ]),
    false,
  );
});

test('writes invalidate their dependencies without flushing unrelated features', () => {
  assert.equal(mutationAffects('/api/dsp/paycom/settings', '/api/dsp/employees/A'), true);
  assert.equal(mutationAffects('/api/dsp/uniforms/stock/A', '/api/dsp/employees/A'), false);
  // A Driver Match decision moves drivers between meal-break rows, and nothing else.
  assert.equal(
    mutationAffects('/api/dsp/driver-match/merge', '/api/dsp/paycom/meal-breaks?date=2026-09-22'),
    true,
  );
  assert.equal(
    mutationAffects('/api/dsp/driver-match/apart', '/api/dsp/driver-match/drivers/K7M2QX'),
    true,
  );
  assert.equal(mutationAffects('/api/dsp/driver-match/split', '/api/dsp/timecards'), false);
  // A finished collection can bring new drivers to match.
  assert.equal(
    collectionAffects('/api/dsp/driver-match', [
      { provider: 'cortex', dates: ['2026-09-22'], employeeCode: null, roster: false },
    ]),
    true,
  );
  assert.equal(mutationAffects('/api/dsp/employees/A/sync', '/api/dsp/employees/A'), true);
  assert.equal(mutationAffects('/api/dsp/employees/A/sync', '/api/dsp/employees/B'), false);
  assert.equal(mutationAffects('/api/session/dsp', '/api/platform/dsps'), false);
});

test('roster changes refresh department options without treating each driver as a settings change', () => {
  const change: CollectionChange = {
    provider: 'paycom',
    dates: [],
    employeeCode: null,
    roster: true,
  };
  assert.equal(collectionAffects('/api/dsp/paycom/settings', [change]), true);
  assert.equal(
    collectionAffects('/api/dsp/paycom/settings', [{ ...change, roster: false }]),
    false,
  );
});
