import http from 'node:http';
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
  // A DSP that doesn't text: what it counts and lists is Slack's alone.
  const phones = { feature: 'phones', enabled: false };
  assert.equal((await owner.post(`/api/platform/dsps/${north.id}/features`, phones)).status, 200);
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

  // Whom the DSP can message, found by a misspelt name, with when they last wrote.
  const { contacts } = await owner.read(`${route}/contacts?q=jordn&team=T0FIXTURE1`);
  assert.equal(contacts[0].person.name, 'Jordan Ellis');
  assert.equal(contacts[0].conversationId, conversation.id);
  assert.equal(contacts[0].lastAt, (await read()).conversations[0].lastAt);
  assert.equal(contacts[0].waitingSince, null);

  // A workspace's lists hold its own alone: answered, older, found, and whom it can message.
  const older = `${route}/conversations?before=${Date.now() + 60_000}:${conversation.id}`;
  for (const [team, ids] of [
    ['T0FIXTURE1', [conversation.id]],
    ['T0ELSEWHERE', []],
  ] as const) {
    const listed = (path: string) => owner.read(`${path}team=${team}`);
    const of = async (path: string) =>
      ((await listed(path)).conversations as { id: string }[]).map(({ id }) => id);
    assert.deepEqual(await of(`${route}?`), ids);
    assert.deepEqual(await of(`${older}&`), ids);
    assert.deepEqual(await of(`${route}/conversations?q=jordn&`), ids);
    const reached = await listed(`${route}/contacts?`);
    assert.equal(reached.contacts.length > 0, ids.length > 0);
  }

  // The image of someone's initials is public, at the admin's address, as Slack fetches it.
  const initials = await fetch(`${f.env.DISPATCH_ORIGIN}/communications/initials/1/PO.png`);
  assert.equal(initials.status, 200);
  assert.equal(initials.headers.get('content-type'), 'image/png');
});

test('Communications answers only those whose role gives them it, and the profile, logos and groups with it alone', async (t) => {
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

  // And the workspace's logo, the copy Slack keeps of it, though they don't manage Slack.
  let logo = '';
  await until(async () => {
    logo = (await member.read(route)).workspaces[0].logo ?? '';
    return Boolean(logo);
  });
  // At the DSP's own address, where the member signs in, which `fetch` can't name.
  const picture = await new Promise<http.IncomingMessage>((resolve, reject) =>
    http
      .get(
        `${f.env.DISPATCH_ORIGIN}${route}/workspaces/T0FIXTURE1/logo?v=${logo}`,
        { headers: member.headers },
        resolve,
      )
      .on('error', reject),
  );
  picture.resume();
  assert.equal(picture.statusCode, 200);
  assert.equal(picture.headers['content-type'], 'image/png');

  // They message a group: whom it can go to, then each person gets it in the background.
  const choices = await member.read(`${route}/groups/choices`);
  const jordan = choices.people.find(
    (each: { person: { name: string } }) => each.person.name === 'Jordan Ellis',
  );
  const made = await member.post(`${route}/groups`, {
    name: 'Jordan Ellis',
    people: [jordan.members[0]],
    text: 'Station is behind',
  });
  assert.equal(made.status, 201, made.body);
  const group = `${route}/groups/${made.value.id}`;
  await until(async () => (await member.read(group)).entries[0]?.progress.sent === 1);
  const { people } = await member.read(`${group}/people`);
  assert.deepEqual(
    people.map((each: { person: { name: string }; state: string }) => [
      each.person.name,
      each.state,
    ]),
    [['Jordan Ellis', 'sent']],
  );
  assert.equal((await member.read(route)).groups[0].id, made.value.id);
});

test('a DSP’s texts are its own: listed by number, answered through its phone as written, and only for those whose role gives them Communications', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const [north, summit] = ['Northline Logistics', 'Summit Delivery'].map((name) =>
    owner.session.dsps.find((d: { name: string }) => d.name === name),
  );
  await owner.select(north.id);
  // The demo's stand-in phone received three texts: each number's conversation waits.
  type Listed = { id: string; kind: string; number: string; state: string };
  let texts: Listed[] = [];
  await until(async () => {
    const listed: Listed[] = (await owner.read(route)).conversations;
    texts = listed.filter((each) => each.kind === 'number');
    return texts.length === 3;
  });
  assert.deepEqual(new Set(texts.map((each) => each.state)), new Set(['waiting']));
  const asked = texts.find((each) => each.number === '+14255550199')!;

  // Answered, the text goes through the phone as written, which delivers it, under who wrote it.
  const answer = await owner.post(`${route}/conversations/${asked.id}/messages`, {
    text: 'Yes, this is dispatch',
  });
  assert.equal(answer.status, 201, answer.body);
  assert.deepEqual(
    [answer.value.text, answer.value.status, answer.value.sender.name],
    ['Yes, this is dispatch', 'delivered', 'Platform Owner'],
  );
  const opened = await owner.read(`${route}/conversations/${asked.id}`);
  assert.equal(opened.conversation.state, 'responded');
  const saved = await owner.post(`${route}/conversations/${asked.id}/contact`, {
    name: 'New hire',
  });
  assert.equal(saved.value.with.name, 'New hire');

  // Another DSP has none of it.
  await owner.select(summit.id);
  assert.equal((await owner.get(`${route}/conversations/${asked.id}`)).status, 404);
  assert.equal((await owner.post(`${route}/conversations/${asked.id}/block`)).status, 404);
  const { contacts } = await owner.read(`${route}/texts/contacts`);
  assert.equal(
    contacts.some((each: { person: { name: string } }) => each.person.name === 'New hire'),
    false,
  );

  // No default role holds communications.use, so a member is refused texts too.
  const member = await f.client(demo.member);
  await member.select(north.id);
  const refused = [
    await member.get(`${route}/texts/contacts`),
    await member.get(`${route}/conversations/${asked.id}`),
    await member.post(`${route}/conversations/${asked.id}/messages`, { text: 'Hi' }),
    await member.post(`${route}/conversations/${asked.id}/contact`, { name: 'Someone' }),
    await member.post(`${route}/conversations/${asked.id}/block`),
    await member.post(`${route}/texts/conversations`, { number: '4255550123' }),
  ];
  assert.deepEqual(
    refused.map((each) => each.status),
    refused.map(() => 403),
  );
});
