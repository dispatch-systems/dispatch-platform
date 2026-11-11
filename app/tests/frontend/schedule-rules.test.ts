import test from 'node:test';
import assert from 'node:assert/strict';
import { withPattern } from '../../../core/shell/frontend/lib/schedule-rules.js';
import {
  newRule as collectionRule,
  finished as collectionInput,
} from '../../../core/collection/frontend/collectors/rules.js';
import {
  newRule as backupRule,
  finished as backupInput,
} from '../../../core/platform_owner/frontend/dsps/backupRules.js';

test('switching timing preserves the collection and monthly backup choices sent to the server', () => {
  const collection = { ...collectionRule('paycom', 'collection'), days: [1, 3], times: ['06:00'] };
  const repeated = withPattern(collection, 'repeat');
  assert.deepEqual(collectionInput({ ...repeated, to: '22:00' }), {
    id: null,
    collects: 'paycom',
    kind: 'repeat',
    days: [1, 3],
    times: [],
    every: 60,
    from: '06:00',
    to: '22:00',
  });
  assert.deepEqual(withPattern(repeated, 'few').times, ['06:00', '']);

  const backup = {
    ...backupRule('backup'),
    on: 'month' as const,
    dates: [15, 1],
    lastDay: true,
    times: ['02:00'],
    keep: 'last' as const,
    keepCount: 12,
    skipped: ['pictures'],
  };
  const input = backupInput({ ...withPattern(backup, 'repeat'), to: '05:00' });
  assert.deepEqual(input, {
    id: null,
    on: 'month',
    days: [],
    dates: [1, 15],
    lastDay: true,
    kind: 'repeat',
    times: [],
    every: 60,
    from: '02:00',
    to: '05:00',
    keep: 'last',
    keepCount: 12,
    keepUnit: null,
    skipped: ['pictures'],
  });
});

test('both editors reject unfinished or duplicate times and sort finished times', () => {
  const collection = collectionRule('paycom', 'collection');
  const backup = backupRule('backup');
  const cases = [
    (times: string[]) => collectionInput({ ...collection, times }),
    (times: string[]) => backupInput({ ...backup, times }),
  ];
  for (const apply of cases) {
    assert.equal(apply(['']), undefined);
    assert.equal(apply(['02:00', '02:00']), undefined);
    assert.deepEqual(apply(['14:00', '06:00'])?.times, ['06:00', '14:00']);
  }
  assert.equal(
    collectionInput({ ...collectionRule('paycom', 'c'), days: [], times: ['02:00'] }),
    undefined,
  );
  assert.equal(
    backupInput({ ...backupRule('b'), on: 'month', dates: [], lastDay: false, times: ['02:00'] }),
    undefined,
  );
});
