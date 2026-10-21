import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';
import { connect, event, fromSlack } from '../../../slack/tests/support/slack.js';

type Fixture = Awaited<ReturnType<typeof fixture>>;

const route = '/api/dsp/communications';
/** Slack's `ts` for now: seconds, a point and microseconds. */
const now = () => (Date.now() / 1000).toFixed(6);
/** Jordan Ellis writing to the app in Operations, as Slack tells of it. */
const fromJordan = (f: Fixture, fields: Record<string, unknown>) =>
  fromSlack(
    f,
    '/slack/events',
    event('T0FIXTURE1', {
      type: 'message',
      channel_type: 'im',
      channel: 'D0APITEST',
      user: 'U0FIXJORDAN',
      ts: now(),
      ...fields,
    }),
  );

test('a direct message reaches Communications live, and its answer and photo go back', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  assert.equal((await connect(owner, 'T0FIXTURE1')).status, 200);
  const write = (fields: Record<string, unknown>) => fromJordan(f, fields);
  const read = () => owner.read(route);

  // A member writes to the app, and the page waiting on changes hears of it.
  const before = await read();
  const waiting = owner.read(
    `${route}/updates?after=${before.revision}&typing=${before.typing.version}`,
  );
  assert.equal((await write({ text: 'Running late' })).status, 200);
  const heard = await waiting;
  assert.equal(heard.conversations[0]?.with.name, 'Jordan Ellis');
  const [conversation] = (await read()).conversations;
  assert.equal(conversation.state, 'waiting');
  assert.equal(conversation.last.text, 'Running late');
  assert.equal((await owner.read(`${route}/waiting`)).count, 1);

  // A photo they send is kept in Dispatch, which serves it.
  await write({
    subtype: 'file_share',
    text: '',
    files: [{ id: 'F0FIXDMPHOTO', name: 'Flat tire.png', mimetype: 'image/png', size: 69 }],
  });
  let photo = '';
  await until(async () => {
    const { messages } = await owner.read(`${route}/conversations/${conversation.id}`);
    const file = messages.at(-1)?.files[0];
    photo = file?.status === 'kept' ? file.id : '';
    return Boolean(photo);
  });
  const bytes = await f.raw(`${route}/files/${photo}`, undefined, owner.headers);
  assert.equal(bytes.status, 200);
  assert.equal(bytes.headers.get('content-type'), 'image/png');
  assert.deepEqual([...new Uint8Array(await bytes.arrayBuffer()).slice(0, 4)], [137, 80, 78, 71]);

  // Answered, it waits no more; written to again, it waits; marked done, it doesn't.
  const answer = await owner.post(`${route}/conversations/${conversation.id}/messages`, {
    text: 'On my way',
  });
  assert.equal(answer.status, 201, answer.body);
  assert.equal(answer.value.sender.name, 'Platform Owner');
  assert.equal((await read()).conversations[0].state, 'responded');
  await write({ text: 'Thanks' });
  await until(async () => (await read()).waiting === 1);
  const done = await owner.post(`${route}/conversations/${conversation.id}/done`);
  assert.equal(done.value.state, 'done');
  assert.equal((await read()).waiting, 0);

  // Whom the DSP can message, found by a misspelt name.
  const { contacts } = await owner.read(`${route}/contacts?q=jordn`);
  assert.equal(contacts[0].person.name, 'Jordan Ellis');
  assert.equal(contacts[0].conversationId, conversation.id);

  // The image of someone's initials is public, at the admin's address, as Slack fetches it.
  const initials = await fetch(`${f.env.DISPATCH_ORIGIN}/communications/initials/1/PO.png`);
  assert.equal(initials.status, 200);
  assert.equal(initials.headers.get('content-type'), 'image/png');
});

test('Communications answers only those whose role gives them it, and the profile with it alone', async (t) => {
  const f = await fixture();
  t.after(f.close);
  // No default role holds communications.use, so a member is refused until a role grants it.
  const refused = await f.client(demo.member);
  const north = refused.session.dsps[0];
  await refused.select(north.id);
  assert.equal((await refused.get(route)).status, 403);

  // Granted it alone, they read the profile of whom a conversation is with, which shows what
  // other features know of them without their permissions.
  const owner = await f.client();
  await owner.select(north.id);
  assert.equal((await connect(owner, 'T0FIXTURE1')).status, 200);
  assert.equal((await fromJordan(f, { text: 'Running late' })).status, 200);
  const role = await owner.post('/api/dsp/roles', {
    name: 'Dispatcher',
    permissions: ['communications.use'],
  });
  const members = (await owner.get('/api/dsp/members')).value;
  const memberId = members.find((m: { email: string }) => m.email === demo.member).id;
  await owner.post(`/api/dsp/members/${memberId}`, { role: role.value.id });
  const member = await f.client(demo.member);
  await member.select(north.id);
  const [conversation] = (await member.read(route)).conversations;
  const profile = await member.read(`${route}/conversations/${conversation.id}/profile`);
  assert.equal(profile.email, 'jordan.ellis@example.com');
  assert.equal(profile.slack, 'Operations');
});
