import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';

// A phone pairs at its DSP's own address with a code someone who manages the DSP's connections
// made, and then speaks only with the key it was given, there and nowhere else.

test('A phone pairs once with a code, then calls with its key at its DSP alone', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const before = await owner.get('/api/dsp/phones');
  assert.equal(before.status, 200, before.body);

  const pairing = await owner.post('/api/dsp/phones/pairing');
  assert.equal(pairing.status, 200, pairing.body);
  assert.match(pairing.value.code, /^[A-HJ-NP-Z2-9]{8}$/);
  assert.match(pairing.value.picture, /^data:image\/svg\+xml;base64,/);

  const at = f.at(demo.memberDsp);
  const phone = {
    code: pairing.value.code,
    number: '(206) 555-0177',
    model: 'Pixel 8a',
    version: '1.0.0',
  };
  // Only at a DSP's own address, and never from a browser.
  assert.equal((await f.request('/api/phones/pair', phone)).status, 401);
  const browser = await f.request('/api/phones/pair', phone, {
    ...at,
    cookie: 'dispatch_session=x',
  });
  assert.equal(browser.status, 401);
  const paired = await f.request('/api/phones/pair', phone, at);
  assert.equal(paired.status, 200, paired.body);
  assert.match(paired.value.key, /^[0-9a-f]{64}$/);
  assert.equal(paired.value.dsp.name, 'Northline Logistics');
  const again = await f.request('/api/phones/pair', phone, at);
  assert.equal(again.value.error, 'phones_pairing_invalid');

  const listed = await owner.get('/api/dsp/phones');
  const row = listed.value.phones.find((p: { id: string }) => p.id === paired.value.phone.id);
  assert.equal(row.number, '+12065550177');
  assert.equal(row.online, true);

  const renamed = await owner.post(`/api/dsp/phones/${paired.value.phone.id}/rename`, {
    name: 'Station 2',
  });
  assert.equal(renamed.status, 200, renamed.body);
  assert.equal(renamed.value.name, 'Station 2');

  const key = { authorization: `Bearer ${paired.value.key}` };
  const text = {
    clientId: 'sms-1',
    direction: 'in',
    number: '4255550199',
    body: 'Gate code?',
    at: Date.now(),
  };
  const reported = await f.request('/api/phones/texts', { texts: [text] }, { ...at, ...key });
  assert.equal(reported.status, 200, reported.body);
  assert.equal(reported.value.texts[0].clientId, 'sms-1');
  // The key works at its DSP's address alone, and only while the phone is paired.
  const elsewhere = await f.request('/api/phones/texts', { texts: [text] }, key);
  assert.equal(elsewhere.status, 401);
  const wrong = { authorization: `Bearer ${'0'.repeat(64)}` };
  assert.equal(
    (await f.request('/api/phones/texts', { texts: [text] }, { ...at, ...wrong })).status,
    401,
  );

  const switches = `/api/platform/dsps/${north.id}/features`;
  assert.equal((await owner.post(switches, { feature: 'phones', enabled: false })).status, 200);
  await owner.select(north.id);
  assert.equal((await owner.get('/api/dsp/phones')).status, 404);
  assert.equal((await owner.post('/api/dsp/phones/pairing')).status, 404);
  assert.equal((await owner.post(switches, { feature: 'phones', enabled: true })).status, 200);
  await owner.select(north.id);

  const removed = await owner.post(`/api/dsp/phones/${paired.value.phone.id}/remove`);
  assert.equal(removed.status, 200, removed.body);
  const after = await f.request(
    '/api/phones/statuses',
    { statuses: [{ id: 'text_x', attempt: 1, status: 'sent', at: 1 }] },
    { ...at, ...key },
  );
  assert.equal(after.status, 401);
  assert.equal(after.value.error, 'phones_removed');
});

test("Only those who manage the DSP's phones or connections see or pair them", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/phones')).status, 403);
  assert.equal((await member.post('/api/dsp/phones/pairing')).status, 403);
});
