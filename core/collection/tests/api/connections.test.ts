import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fixture } from '../../../shell/tests/support/support.js';

const credentials = {
  clientCode: 'TEST',
  username: 'private-user',
  password: 'synthetic-password',
  securityAnswers: ['00123', 'two', 'three', 'four', 'five'],
};

// Core's own routes: a DSP's connections, and the platform owner's list of every DSP's jobs.
// The jobs and schedules a feature mounts are tested with that feature.
test("a DSP's connections check and encrypt credentials, and only the platform owner lists every job", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get('/api/platform/jobs')).status, 403);
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/connections')).status, 403);
  const jobs = await owner.get('/api/platform/jobs');
  assert.equal(jobs.status, 200);
  assert(Array.isArray(jobs.value));
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(dsp.id);
  assert.equal(
    (
      await owner.post('/api/dsp/connections/paycom', {
        ...credentials,
        securityAnswers: ['1', '1', '3', '4', '5'],
      })
    ).status,
    400,
  );
  const saved = await owner.post('/api/dsp/connections/paycom', {
    ...credentials,
    password: 'require-verification',
  });
  assert.equal(saved.status, 200, JSON.stringify(saved.value));
  assert.equal(saved.value.status, 'needs_verification');
  const vault = path.join(f.root, 'dsps', dsp.id, 'secrets/paycom.enc');
  assert(!fs.readFileSync(vault, 'utf8').includes('private-user'));
  assert.equal(fs.statSync(vault).mode & 0o077, 0);
});
