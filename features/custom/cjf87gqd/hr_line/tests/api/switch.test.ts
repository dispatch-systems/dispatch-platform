import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../../../core/shell/tests/support/support.js';
import { dispatchPhone, hrLine, paired, withHrLine } from '../support/line.js';

// HR Line answers to its own switch alone, never to Dispatch Phone's: its page, its card, its
// phones and the codes that pair them work whether Dispatch Phone is on, off or in beta, and only
// while HR Line itself is on. The Relay app its phones run is all the two share.

const app = '/api/communications/phone/app';

test('HR Line works with Dispatch Phone off, and only while its own switch is on', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = await withHrLine(owner);
  const switches = `/api/platform/dsps/${north.id}/features`;
  const turn = async (feature: string, enabled: boolean) => {
    assert.equal((await owner.post(switches, { feature, enabled })).status, 200);
    await owner.select(north.id);
  };
  const at = f.at('nll');
  const dispatch = await paired(f, owner, dispatchPhone, '(206) 555-0177');

  // With Dispatch Phone off, HR Line pairs a phone, which relays a text that its page lists, and
  // its answer goes from that phone; the address still hands out the app. Dispatch Phone's own
  // phone is refused.
  await turn('communications', false);
  const hr = await paired(f, owner, hrLine, '(206) 555-0111');
  await hr.texted('4255550199', 'About my paycheck');
  let conversation = '';
  await until(async () => {
    const listed: { id: string; number: string | null }[] = (await owner.read(hrLine))
      .conversations;
    conversation = listed.find((each) => each.number === '+14255550199')?.id ?? '';
    return conversation !== '';
  });
  const answered = await owner.post(`${hrLine}/conversations/${conversation}/messages`, {
    text: 'Payroll will call you',
  });
  assert.equal(answered.status, 201, answered.body);
  const outbox = await f.request('/api/dispatch-phone/phone/outbox', undefined, hr.key);
  assert.deepEqual(
    outbox.value.texts.map((each: { body: string }) => each.body),
    ['Payroll will call you'],
  );
  assert.equal((await f.request(app, undefined, at)).value.error, 'phones_app_not_found');
  const asleep = await f.request('/api/dispatch-phone/phone/outbox', undefined, dispatch.key);
  assert.equal(asleep.value.error, 'phones_off');
  assert.equal((await owner.get(`${dispatchPhone}/phones`)).status, 404);
  const code = await owner.post(`${hrLine}/phones/pairing`);
  assert.equal(code.status, 200, code.body);

  // With Dispatch Phone on and HR Line off, HR Line's page, card, phone and code are refused, and
  // Dispatch Phone's phone works again.
  await turn('communications', true);
  await turn('cjf87gqd_hr_line', false);
  assert.equal((await owner.get(hrLine)).status, 403);
  assert.equal((await owner.get(`${hrLine}/phones`)).status, 404);
  const refused = await f.request('/api/dispatch-phone/phone/outbox', undefined, hr.key);
  assert.equal(refused.value.error, 'phones_off');
  const phone = {
    code: code.value.code,
    number: '(206) 555-0112',
    model: 'Pixel 8a',
    version: '1',
  };
  const unpaired = await f.request('/api/dispatch-phone/phone/pair', phone, at);
  assert.equal(unpaired.value.error, 'phones_off');
  await dispatch.texted('4255550123', 'Is this dispatch?');

  // With neither on, the address hands out nothing.
  await turn('communications', false);
  assert.equal((await f.request(app, undefined, at)).value.error, 'phones_off');
});

test('Those who use HR Line have it while Dispatch Phone is in beta and they test none of it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = await withHrLine(owner);
  const role = await owner.post('/api/dsp/roles', {
    name: 'HR',
    permissions: ['cjf87gqd_hr_line.use', 'cjf87gqd_hr_line.phones'],
  });
  assert.equal(role.status, 201, role.body);
  const members = (await owner.get('/api/dsp/members')).value;
  const memberId = members.find((m: { email: string }) => m.email === demo.member).id;
  await owner.post(`/api/dsp/members/${memberId}`, { role: role.value.id });
  const visibility = `/api/platform/dsps/${north.id}/features/visibility`;
  const beta = await owner.post(visibility, { feature: 'communications', visibility: 'beta' });
  assert.equal(beta.status, 200, beta.body);

  // A member whose role gives them HR Line, who tests nothing, has its page and its card.
  const member = await f.client(demo.member);
  const view = await member.select(north.id);
  assert.ok(!view.features.includes('communications'));
  assert.equal((await member.get(hrLine)).status, 200);
  assert.equal((await member.get(`${hrLine}/phones`)).status, 200);

  // So does the platform owner looking through the DSP's Owner role.
  const opened = await owner.select(north.id);
  const ownerRole = opened.roles.find((each: { owner: boolean }) => each.owner);
  const preview = await owner.post('/api/session/dsp', { dspId: north.id, roleId: ownerRole.id });
  assert.equal(preview.status, 200, preview.body);
  owner.headers['x-dispatch-view'] = preview.value.token;
  assert.ok(!preview.value.features.includes('communications'));
  assert.equal((await owner.get(hrLine)).status, 200);
  assert.equal((await owner.get(`${hrLine}/phones`)).status, 200);
});

test("A member's Display choices are both lines', made in either with the other's switch off", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = await withHrLine(owner);
  const turn = async (feature: string, enabled: boolean) => {
    const turned = await owner.post(`/api/platform/dsps/${north.id}/features`, {
      feature,
      enabled,
    });
    assert.equal(turned.status, 200, turned.body);
    await owner.select(north.id);
  };
  type Held = { id: string; values: Record<string, unknown>; revision: number };
  const held = async (id: string) =>
    ((await owner.read('/api/dsp/settings')).sections as Held[]).find((each) => each.id === id);
  const choose = async (id: string, values: Record<string, unknown>) => {
    const saved = await owner.post(`/api/dsp/settings/${id}`, {
      values,
      revision: (await held(id))!.revision,
    });
    assert.equal(saved.status, 200, saved.body);
  };
  const dispatch = 'communications.display';
  const hr = 'cjf87gqd_hr_line.display';

  // With Dispatch Phone off, HR Line's choices are made, and are Dispatch Phone's once it's on.
  await turn('communications', false);
  assert.equal(await held(dispatch), undefined);
  await choose(hr, { text_size: 17, notifications: true });
  await turn('communications', true);
  assert.deepEqual((await held(dispatch))!.values, (await held(hr))!.values);
  assert.equal((await held(dispatch))!.values.text_size, 17);

  // With HR Line off, Dispatch Phone's are made, and are HR Line's once it's on, the next save of
  // either on the revision both hold.
  await turn('cjf87gqd_hr_line', false);
  assert.equal(await held(hr), undefined);
  await choose(dispatch, { text_size: 13, notifications: false });
  await turn('cjf87gqd_hr_line', true);
  const [mine, theirs] = [(await held(dispatch))!, (await held(hr))!];
  assert.deepEqual(theirs.values, mine.values);
  assert.equal(theirs.values.text_size, 13);
  assert.equal(theirs.revision, mine.revision);
  await choose(hr, { layout: 'centred' });
  assert.equal((await held(dispatch))!.values.layout, 'centred');
});
