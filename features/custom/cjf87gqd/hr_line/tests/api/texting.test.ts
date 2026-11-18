import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../../../core/shell/tests/support/support.js';
import { dispatchPhone, hrLine, paired, withHrLine } from '../support/line.js';

// HR Line's page reads and answers HR Line's texts alone, from its own phone, behind Use HR Line:
// a number texting both lines has a conversation on each, and neither page lists, opens, answers,
// names or blocks the other's.

type Listed = { id: string; number: string | null; with: { name: string }; last: { text: string } };
const JORDAN = '+14255550199';

test("HR Line's page has HR Line's texts alone, and Dispatch Phone's page none of them", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  await withHrLine(owner);
  const hr = await paired(f, owner, hrLine, '(206) 555-0111');
  const dispatch = await paired(f, owner, dispatchPhone, '(206) 555-0177');
  await hr.texted('4255550199', 'About my paycheck');
  await dispatch.texted('4255550199', 'Is this dispatch?');

  const listed = async (base: string) =>
    ((await owner.read(base)).conversations as Listed[]).filter((each) => each.number === JORDAN);
  let ours: Listed[] = [];
  await until(async () => (ours = await listed(hrLine)).length > 0);
  let theirs: Listed[] = [];
  await until(async () => (theirs = await listed(dispatchPhone)).length > 0);
  // One conversation with the number on each, each with its own line's texts.
  assert.deepEqual(
    ours.map((each) => each.last.text),
    ['About my paycheck'],
  );
  assert.deepEqual(
    theirs.map((each) => each.last.text),
    ['Is this dispatch?'],
  );
  const [mine, other] = [ours[0]!, theirs[0]!];
  assert.equal((await owner.read(`${hrLine}/waiting`)).count, 1);
  assert.deepEqual(
    (await owner.read(`${hrLine}/conversations?q=4255550199&kind=number`)).conversations.map(
      (each: Listed) => each.id,
    ),
    [mine.id],
  );

  // Neither opens, answers, marks done, names or blocks the other's.
  for (const [base, id] of [
    [hrLine, other.id],
    [dispatchPhone, mine.id],
  ] as const) {
    assert.equal((await owner.get(`${base}/conversations/${id}`)).status, 404);
    for (const [path, body] of [
      ['messages', { text: 'Hi' }],
      ['done', {}],
      ['contact', { name: 'Someone' }],
      ['block', {}],
    ] as const) {
      const refused = await owner.post(`${base}/conversations/${id}/${path}`, body);
      assert.equal(refused.value.error, 'communications_conversation_not_found', path);
    }
  }

  // HR Line's answer goes from its own phone; the name it saves and the number it blocks are its
  // own.
  const answered = await owner.post(`${hrLine}/conversations/${mine.id}/messages`, {
    text: 'Payroll will call you',
  });
  assert.equal(answered.status, 201, answered.body);
  const outbox = await f.request('/api/dispatch-phone/phone/outbox', undefined, hr.key);
  assert.deepEqual(
    outbox.value.texts.map((each: { body: string }) => each.body),
    ['Payroll will call you'],
  );
  const named = await owner.post(`${hrLine}/conversations/${mine.id}/contact`, {
    name: 'Jordan Ellis',
  });
  assert.equal(named.value.with.name, 'Jordan Ellis');
  assert.equal((await owner.post(`${hrLine}/conversations/${mine.id}/block`, {})).status, 200);
  const blocked = (base: string) =>
    owner
      .read(`${base}/texts/contacts`)
      .then(({ blocked }) => blocked.map((each: { number: string }) => each.number));
  assert.deepEqual(await blocked(hrLine), [JORDAN]);
  assert.deepEqual(await blocked(dispatchPhone), []);
  const still = await owner.read(`${dispatchPhone}/conversations/${other.id}`);
  assert.equal(still.conversation.with.name, '(425) 555-0199');
});

test('HR Line answers only those whose role gives them Use HR Line', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = await withHrLine(owner);
  const refused = await f.client(demo.member);
  await refused.select(north.id);
  assert.equal((await refused.get(hrLine)).status, 403);
  assert.equal((await refused.get(`${hrLine}/texts/contacts`)).status, 403);

  const role = await owner.post('/api/dsp/roles', {
    name: 'HR',
    permissions: ['cjf87gqd_hr_line.use'],
  });
  assert.equal(role.status, 200, role.body);
  const members = (await owner.get('/api/dsp/members')).value;
  const memberId = members.find((m: { email: string }) => m.email === demo.member).id;
  await owner.post(`/api/dsp/members/${memberId}`, { role: role.value.id });
  const member = await f.client(demo.member);
  await member.select(north.id);
  assert.equal((await member.get(hrLine)).status, 200);
  // Using HR Line is not using Dispatch Phone.
  assert.equal((await member.get(dispatchPhone)).status, 403);
});
