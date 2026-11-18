import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../../../../core/shell/tests/support/support.js';

// HR Line's phones pair, list, rename and remove on its own card's routes, and Dispatch Phone's on
// its own: neither line sees or changes the other's, a code of one never replaces the other's, and
// every other DSP has no HR Line.

const hr = '/api/dsp/cjf87gqd-hr-line/phones';
const dispatch = '/api/dsp/dispatch-phone/phones';

test("HR Line's phones and Dispatch Phone's are each managed only on their own card", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  assert.equal(
    (await owner.post(`/api/platform/dsps/${north.id}/custom-id`, { customId: 'cjf87gqd' })).status,
    200,
  );
  const on = await owner.post(`/api/platform/dsps/${north.id}/features`, {
    feature: 'cjf87gqd_hr_line',
    enabled: true,
  });
  assert.equal(on.status, 200, on.body);
  await owner.select(north.id);
  const pair = async (code: string, number: string) => {
    const phone = { code, number, model: 'Pixel 8a', version: '1.0.0' };
    const paired = await f.request('/api/dispatch-phone/phone/pair', phone, f.at('nll'));
    assert.equal(paired.status, 200, paired.body);
    return { ...paired.value.phone, line: paired.value.line.name } as {
      id: string;
      name: string;
      line: string;
    };
  };
  const code = async (base: string) => {
    const made = await owner.post(`${base}/pairing`);
    assert.equal(made.status, 200, made.body);
    return made.value as { id: string; code: string };
  };
  const listed = async (base: string) => {
    const read = await owner.get(base);
    assert.equal(read.status, 200, read.body);
    return read.value.phones.map((phone: { id: string }) => phone.id);
  };

  // HR Line's code, made first, still pairs once Dispatch Phone's is made, each to its own line.
  const hrCode = await code(hr);
  const dispatchCode = await code(dispatch);
  assert.equal((await owner.get(`${dispatch}/pairing/${hrCode.id}`)).value.status, 'expired');
  const hrPhone = await pair(hrCode.code, '(206) 555-0111');
  const dispatchPhone = await pair(dispatchCode.code, '(206) 555-0177');
  assert.equal(hrPhone.name, 'HR Line');
  // Each phone is told the line it serves.
  assert.equal(hrPhone.line, 'HR Line');
  assert.equal(dispatchPhone.line, 'Dispatch Phone');
  assert.deepEqual(await listed(hr), [hrPhone.id]);
  const dispatchPhones = await listed(dispatch);
  assert.ok(dispatchPhones.includes(dispatchPhone.id));
  assert.ok(!dispatchPhones.includes(hrPhone.id));

  // Neither card renames or removes the other's.
  const renamed = await owner.post(`${dispatch}/${hrPhone.id}/rename`, { name: 'Mine' });
  assert.equal(renamed.value.error, 'phones_not_found');
  const removed = await owner.post(`${hr}/${dispatchPhone.id}/remove`);
  assert.equal(removed.value.error, 'phones_not_found');
  assert.ok((await listed(dispatch)).includes(dispatchPhone.id));
  const ours = await owner.post(`${hr}/${hrPhone.id}/rename`, { name: 'HR Front Desk' });
  assert.equal(ours.status, 200, ours.body);
  assert.equal(ours.value.name, 'HR Front Desk');

  // Every other DSP has no HR Line.
  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  await owner.select(summit.id);
  assert.equal((await owner.get(hr)).status, 404);
});
