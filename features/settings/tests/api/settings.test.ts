import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../core/shell/tests/support/support.js';

test("a DSP's profile is saved by whoever manages its settings, and by nobody else", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  const profile = {
    name: north.name,
    timezone: north.timezone,
    abbreviation: 'NLL',
    stationCode: 'tst1',
  };
  assert.equal((await f.request('/api/dsp/profile', profile)).status, 401);
  // A member's role holds no settings.manage.
  await member.select(north.id);
  const refused = await member.post('/api/dsp/profile', profile);
  assert.deepEqual([refused.status, refused.value.error], [403, 'permission_denied']);
  await owner.select(north.id);
  const invalid = await owner.post('/api/dsp/profile', {});
  assert.deepEqual([invalid.status, invalid.value.error], [400, 'invalid_input']);
  const saved = await owner.post('/api/dsp/profile', profile);
  assert.deepEqual([saved.status, saved.value], [200, { ok: true }]);
  // The station code is kept as Amazon writes it.
  assert.equal((await owner.select(north.id)).profile.stationCode, 'TST1');
});
