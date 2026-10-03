import test from 'node:test';
import assert from 'node:assert/strict';
import type { AuditEvent } from '../../../../shared/contracts/index.js';
import {
  changeValue,
  installWording,
  notes,
  plain,
  sentence,
} from '../../frontend/audit/wording.js';

const event = (action: string, changes: [string, string][] = []): AuditEvent => ({
  id: 1,
  at: '2026-09-03T10:00:00Z',
  actorId: null,
  actorName: 'System',
  dspId: 'dsp_fixture',
  dspName: 'Fixture Delivery',
  action,
  detail: '',
  area: 'collections',
  target: null,
  ref: null,
  changes: changes.map(([field, to]) => ({ field, from: null, to })),
});

// A stand-in owner's wording, as the audit log's loader hands it over.
installWording([
  {
    collected: { fixture: 'Fixture' },
    value: (field, value) => (field === 'collection' && value === 'all' ? 'Everything' : undefined),
    notes: (e) => (e.action === 'collection.completed' ? ['Fixture note'] : []),
  },
]);

test("an owner names its collections' outcomes, and its notes follow the log's own", () => {
  const completed = event('collection.completed', [
    ['provider', 'fixture'],
    ['date', '2026-09-03'],
    ['duration', '65'],
  ]);
  assert.equal(plain(sentence(completed)), 'Fixture collection for Sep 3 completed');
  assert.deepEqual(notes(completed, false), ['1m 5s', 'Fixture note']);
  assert.equal(
    plain(sentence(event('collection.completed', [['provider', 'other']]))),
    'Collection completed',
  );
});

test("a schedule's collection reads as its owner words it, else as its name", () => {
  assert.equal(changeValue('collection', 'all'), 'Everything');
  assert.equal(changeValue('collection', 'some_data'), 'Some Data');
});
