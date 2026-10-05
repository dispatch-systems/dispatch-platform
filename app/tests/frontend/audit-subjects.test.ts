import assert from 'node:assert/strict';
import test from 'node:test';
import { auditPageSchema } from '../../../core/platform_owner/api/runtime.js';
import { auditSubjects } from '../../../core/tenancy/api/generated/audit-subjects.js';

const page = (kind: string) => ({
  events: [
    {
      id: 1,
      at: '2026-10-05T12:00:00Z',
      actorId: null,
      actorName: 'Owner',
      dspId: null,
      dspName: null,
      action: 'changed',
      detail: '',
      area: 'team',
      target: 'Example',
      ref: { kind, id: 'example' },
      changes: [],
    },
  ],
  total: 1,
  counts: {},
  actors: [],
  dsps: [],
});

test('the audit log accepts an event about every kind of record core and the features name', () => {
  for (const kind of auditSubjects) {
    assert.doesNotThrow(() => auditPageSchema.parse(page(kind)), kind);
  }
});

test('the audit log refuses a kind of record nothing names', () => {
  assert.throws(() => auditPageSchema.parse(page('nothing')));
});
