import test from 'node:test';
import assert from 'node:assert/strict';
import type { MailMessage, PlatformDiagnostics, PlatformHealth } from '../../api/index.js';
import { parseApiResponse } from '../../../shell/frontend/runtime/replies.js';
import { issues } from '../../frontend/diagnostics/attention.js';
import { stage } from '../../frontend/diagnostics/mail.js';

const health = (canStart: boolean, failed: number) =>
  ({
    browsers: {
      memory: { canStart, availableBytes: 900 * 1024 ** 2, requiredBytes: 1536 * 1024 ** 2 },
    },
    mail: { failed },
  }) as PlatformHealth;
const diagnostics = (storageAvailableBytes: number): PlatformDiagnostics => ({
  enabled: true,
  storageAvailableBytes,
  runtime: {
    name: '',
    status: 'Running',
    memoryBytes: 0,
    coreMemoryBytes: 0,
    workerMemoryBytes: 0,
    browsers: 0,
  },
  dsps: [],
});

test('Diagnostics validates reads and fixture replies, including large byte counts and DSP records', async () => {
  const empty = diagnostics(0);
  const populated: PlatformDiagnostics = {
    enabled: false,
    storageAvailableBytes: 2 ** 63,
    runtime: {
      name: 'Shared platform (Rust)',
      status: 'Running',
      memoryBytes: 2 ** 63,
      coreMemoryBytes: 2 ** 62,
      workerMemoryBytes: 2 ** 62,
      browsers: 1,
    },
    dsps: (['provisioning', 'active', 'suspended', 'failed'] as const).map((status, index) => ({
      id: `dsp_${index}`,
      name: `Test DSP ${index}`,
      status,
    })),
  };
  for (const method of ['GET', 'POST'] as const) {
    const parse = (value: unknown) => parseApiResponse('/api/platform/diagnostics', method, value);
    for (const value of [empty, populated]) assert.deepEqual(await parse(value), value);
    for (const value of [
      { ...populated, storageAvailableBytes: null },
      { ...populated, runtime: { ...populated.runtime, coreMemoryBytes: undefined } },
      { ...populated, runtime: { ...populated.runtime, workerMemoryBytes: '0' } },
      { ...populated, runtime: { ...populated.runtime, memoryBytes: -1 } },
      { ...populated, runtime: { ...populated.runtime, browsers: 0.5 } },
      { ...populated, dsps: [{ id: null, name: 'Test DSP', status: 'active' }] },
      { ...populated, dsps: [{ id: 'dsp_test', name: 'Test DSP', status: 'unknown' }] },
    ])
      await assert.rejects(parse(value), { message: 'invalid_api_response' });
  }
});

test('a healthy platform needs no attention; memory, storage and failed mail each raise one issue', () => {
  assert.deepEqual(issues(health(true, 0), diagnostics(5 * 1024 ** 3), []), []);
  const found = issues(health(false, 2), diagnostics(200 * 1024 ** 2), []);
  assert.deepEqual(
    found.map((issue) => issue.id),
    ['browser-memory', 'storage', 'email'],
  );
  assert.match(found[0]!.details[0]!, /900 MiB free, 1536 MiB needed/);
  assert.deepEqual(found[2]!.open, { tab: 'email' });
  assert.equal(found[2]!.details[0], '2 failed deliveries');
});

test('an email is undelivered, waiting on the person, or done', () => {
  const message = (fields: Partial<MailMessage>) =>
    ({ status: 'sent', kind: 'invitation', owner: false, ...fields }) as MailMessage;
  assert.equal(stage(message({ status: 'failed' })), 'undelivered');
  assert.equal(stage(message({ status: 'pending' })), 'undelivered');
  assert.equal(stage(message({ kind: 'reset' })), 'done');
  assert.equal(stage(message({ acceptedAt: null })), 'waiting');
  assert.equal(stage(message({ acceptedAt: '2026-09-20T00:00:00Z' })), 'done');
  const owner = { owner: true, acceptedAt: '2026-09-20T00:00:00Z' };
  assert.equal(stage(message({ ...owner, setupComplete: false })), 'waiting');
  assert.equal(stage(message({ ...owner, setupComplete: true })), 'done');
});
