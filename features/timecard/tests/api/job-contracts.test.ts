import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../../core/shell/tests/support/support.js';
import { parseApiResponse } from '../../../../core/shell/frontend/runtime/replies.js';
import { sessionSchema } from '../../../../core/accounts/api/runtime.js';
import { jobSchema } from '../../../../core/collection/api/runtime.js';

test('authentication and job contracts reject malformed values and validate real responses', async (t) => {
  const f = await fixture();
  t.after(f.close);
  for (const body of [
    { email: 'owner@dispatch.test' },
    { email: 'owner@dispatch.test', password: 12 },
    { email: 'owner@dispatch.test', password: 'test', extra: true },
  ])
    assert.equal((await f.request('/api/auth/login', body)).status, 400);
  const owner = await f.client();
  assert(sessionSchema.safeParse(owner.session).success);
  assert.throws(
    () =>
      parseApiResponse('/api/session', 'GET', {
        ...owner.session,
        user: { ...owner.session.user, platformOwner: 'yes' },
      }),
    /invalid_api_response/,
  );
  await owner.select(
    owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics').id,
  );
  for (const body of [
    { requestId: 'test', date: null },
    { requestId: 1 },
    { requestId: 'test', extra: true },
  ])
    assert.equal((await owner.post('/api/dsp/jobs', body)).status, 400);
  const job = (await owner.post('/api/dsp/jobs', { requestId: 'typed-job' })).value;
  assert.equal(jobSchema.parse(job).status, 'queued');
  await until(async () => {
    const jobs = await owner.read('/api/dsp/jobs');
    parseApiResponse('/api/dsp/jobs', 'GET', jobs);
    return jobs.find((j: { id: string }) => j.id === job.id).status === 'succeeded';
  });
  for (const invalid of [
    { ...job, status: 'finished' },
    { ...job, attempt: '0' },
    { ...job, metrics: [{}] },
    { ...job, error: undefined },
  ])
    assert.throws(() => parseApiResponse('/api/dsp/jobs', 'POST', invalid), /invalid_api_response/);
  for (const route of [
    '/api/auth/login',
    '/api/auth/logout',
    '/api/auth/password',
    '/api/auth/reset-password',
    '/api/auth/forgot-password',
  ]) {
    assert.deepEqual(parseApiResponse(route, 'POST', { ok: true }), { ok: true });
    assert.throws(() => parseApiResponse(route, 'POST', { ok: 'yes' }), /invalid_api_response/);
  }
});
