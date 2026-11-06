import test from 'node:test';
import assert from 'node:assert/strict';
import { checkReply } from '../../../../core/foundation/api/runtime.js';
import { replies } from '../../api/runtime.js';
import { syncJob } from '../support/status.js';

test('daily status validates both sources while retaining full jobs and their original dates', () => {
  const path = '/api/dsp/jobs/meal-breaks?date=2026-09-16';
  const empty = {
    date: '2026-09-16',
    scopeAvailable: false,
    paycom: { enabled: false, active: false, jobDate: null, job: null, collectedAt: null },
    flex: { enabled: false, active: false, jobDate: null, job: null, collectedAt: null },
  };
  const running = {
    ...empty,
    scopeAvailable: true,
    paycom: {
      enabled: true,
      active: false,
      jobDate: null,
      job: syncJob('succeeded', 'paycom.collect'),
      collectedAt: '2026-09-16T01:02:03Z',
    },
    flex: {
      enabled: true,
      active: true,
      jobDate: '2026-09-15',
      job: syncJob('waiting_verification', 'cortex.meal_breaks.collect'),
      collectedAt: null,
    },
  };
  const failed = {
    ...running,
    flex: {
      ...running.flex,
      active: false,
      job: { ...running.flex.job, status: 'failed', error: 'collection_failed' },
    },
  };
  for (const value of [empty, running, failed])
    assert.deepEqual(checkReply([replies], path, 'GET', value), value);
  for (const value of [
    { ...running, date: undefined },
    { ...running, scopeAvailable: 'yes' },
    { ...running, flex: { ...running.flex, jobDate: undefined } },
    { ...running, flex: { ...running.flex, job: { status: 'running' } } },
  ])
    assert.throws(() => checkReply([replies], path, 'GET', value), /invalid_api_response/);
  const queued = { date: '2026-09-16', jobs: [running.paycom.job, running.flex.job] };
  assert.deepEqual(checkReply([replies], path, 'POST', queued), queued);
});

test('Paycom status preserves nullable publication and connection fields and optional verification', () => {
  const path = '/api/dsp/paycom/status';
  const empty = {
    connection: {
      provider: 'paycom',
      enabled: false,
      status: 'not_connected',
      error: null,
      updatedAt: '',
      lastVerifiedAt: null,
      accountLabel: null,
    },
    workforce: { collectedAt: null },
  };
  const verifying = {
    connection: {
      ...empty.connection,
      enabled: true,
      status: 'needs_verification',
      error: 'verification_required',
      updatedAt: '2026-09-16T01:02:03Z',
      lastVerifiedAt: '2026-09-15T01:02:03Z',
      accountLabel: 'Fixture account',
      verificationSessionId: 'fixture-session',
    },
    workforce: { collectedAt: '2026-09-15T01:02:03Z' },
  };
  for (const value of [empty, verifying])
    assert.deepEqual(checkReply([replies], path, 'GET', value), value);
  for (const value of [
    { ...empty, workforce: {} },
    { ...empty, connection: { enabled: false, status: 'not_connected' } },
    { ...verifying, connection: { ...verifying.connection, verificationSessionId: null } },
  ])
    assert.throws(() => checkReply([replies], path, 'GET', value), /invalid_api_response/);
});
