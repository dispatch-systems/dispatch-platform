import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';

type Client = Awaited<ReturnType<Awaited<ReturnType<typeof fixture>>['client']>>;

// Fixture mode stands in for Google: its sign-in sends the browser straight back with a
// code naming the account, `fixture:<email>`.
const signIn = (url: string) => {
  const sent = new URL(url).searchParams;
  return { state: sent.get('state')!, code: sent.get('code')! };
};

test('an owner connects Google, reconnects only the same account, and disconnects it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsps = owner.session.dsps as { id: string; name: string }[];
  const north = dsps.find((d) => d.name === 'Northline Logistics')!;
  const summit = dsps.find((d) => d.name === 'Summit Delivery')!;
  await owner.select(north.id);
  assert.equal((await owner.read('/api/dsp/documents')).connection, null);

  const started = await owner.post('/api/dsp/documents/connect');
  assert.equal(started.status, 200, started.body);
  const { state, code } = signIn(started.value.url);
  // A sign-in belongs to the DSP it started in.
  await owner.select(summit.id);
  const elsewhere = await owner.post('/api/dsp/documents/connect/finish', { state, code });
  assert.equal(elsewhere.value.error, 'documents_connect_expired');
  await owner.select(north.id);
  const finished = await owner.post('/api/dsp/documents/connect/finish', { state, code });
  assert.equal(finished.status, 200, finished.body);
  assert.equal(finished.value.connection.status, 'connected');
  assert.equal(finished.value.connection.accountEmail, 'documents@example.com');
  assert.equal(finished.value.connection.accountKind, 'personal');
  assert.equal(finished.value.connection.folderName, 'Northline Logistics Documents');

  // Only the account that holds the folder can reach the files Dispatch made in it.
  const again = signIn((await owner.post('/api/dsp/documents/connect')).value.url);
  const other = await owner.post('/api/dsp/documents/connect/finish', {
    state: again.state,
    code: 'fixture:someone.else@example.com',
  });
  assert.equal(other.value.error, 'documents_account_mismatch');
  assert.equal(
    (await owner.read('/api/dsp/documents')).connection.accountEmail,
    'documents@example.com',
  );

  const disconnected = await owner.post('/api/dsp/documents/disconnect');
  assert.equal(disconnected.status, 200, disconnected.body);
  assert.equal(disconnected.value.connection, null);
});

test('a role given Documents sees it, and only those who manage it connect Google', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  // Every permission starts off, so the owner turns Documents on for the member's role.
  const roles: { id: string; name: string; permissions: string[] }[] = (
    await owner.get('/api/dsp/roles')
  ).value;
  const role = roles.find((each) => each.name === 'Member')!;
  const saved = await owner.post(`/api/dsp/roles/${role.id}`, {
    name: role.name,
    permissions: [...role.permissions, 'documents.use'],
  });
  assert.equal(saved.status, 200, saved.body);
  await member.select(north.id);
  const overview = await member.read('/api/dsp/documents');
  assert.equal(overview.connection, null);
  assert.equal((await member.post('/api/dsp/documents/connect')).status, 403);
  assert.equal((await member.post('/api/dsp/documents/disconnect')).status, 403);
});

/** Connects the DSP open in `client` to fixture mode's Google account. */
async function connect(client: Client) {
  const { state, code } = signIn((await client.post('/api/dsp/documents/connect')).value.url);
  const finished = await client.post('/api/dsp/documents/connect/finish', { state, code });
  assert.equal(finished.status, 200, finished.body);
}

test('a team makes, finds, renames and trashes files, and never reaches another DSP’s', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsps = owner.session.dsps as { id: string; name: string }[];
  const north = dsps.find((d) => d.name === 'Northline Logistics')!;
  const summit = dsps.find((d) => d.name === 'Summit Delivery')!;
  await owner.select(north.id);
  await connect(owner);
  const make = async (kind: string, name: string, folder: string | null = null) => {
    const made = await owner.post('/api/dsp/documents/new', { folder, kind, name });
    assert.equal(made.status, 200, made.body);
    return made.value as { id: string; name: string; kind: string; url: string };
  };
  const safety = await make('folder', 'Safety & Compliance');
  const plan = await make('doc', '  Rescue plan  ', safety.id);
  assert.equal(plan.name, 'Rescue plan');
  assert.match(plan.url, /^https:\/\/docs\.google\.com\/document\/d\//);
  await make('sheet', 'Fuel cards');

  // The top holds the folder first, counting what it holds, then the files; each names who
  // added it through Dispatch, not the account Google says made it.
  const top = (await owner.get('/api/dsp/documents/folder')).value;
  assert.deepEqual(
    top.items.map((item: { name: string; kind: string; items: number | null }) => [
      item.name,
      item.kind,
      item.items,
    ]),
    [
      ['Safety & Compliance', 'folder', 1],
      ['Fuel cards', 'sheet', null],
    ],
  );
  assert.equal(top.items[1].addedBy, 'Platform support');
  assert.equal(top.items[1].modifiedBy, 'Platform support');
  const inside = (await owner.get(`/api/dsp/documents/folder?id=${safety.id}`)).value;
  assert.deepEqual(inside.path, [{ id: safety.id, name: 'Safety & Compliance' }]);
  assert.deepEqual(
    inside.items.map((item: { name: string }) => item.name),
    ['Rescue plan'],
  );
  // A search from the top finds what is inside a folder, and says where.
  const found = (await owner.get('/api/dsp/documents/folder?q=rescue')).value.items;
  assert.deepEqual(
    found.map((item: { name: string; location: string }) => [item.name, item.location]),
    [['Rescue plan', 'Safety & Compliance']],
  );

  const renamed = await owner.post(`/api/dsp/documents/items/${plan.id}/rename`, {
    name: 'Rescue plan October',
  });
  assert.equal(renamed.value.name, 'Rescue plan October', renamed.body);
  assert.equal(
    (await owner.post(`/api/dsp/documents/items/${plan.id}/rename`, { name: ' ' })).value.error,
    'documents_name_invalid',
  );

  // Summit connects the same Google account, and none of Northline's files are its to reach.
  await owner.select(summit.id);
  await connect(owner);
  assert.deepEqual((await owner.get('/api/dsp/documents/folder')).value.items, []);
  for (const [path, body] of [
    [`/api/dsp/documents/folder?id=${safety.id}`, undefined],
    [`/api/dsp/documents/items/${plan.id}/rename`, { name: 'Mine now' }],
    [`/api/dsp/documents/items/${plan.id}/trash`, {}],
    ['/api/dsp/documents/new', { folder: safety.id, kind: 'doc', name: 'Mine now' }],
  ] as const) {
    const refused = body ? await owner.post(path, body) : await owner.get(path);
    assert.equal(refused.value.error, 'documents_item_not_found', path);
  }

  await owner.select(north.id);
  assert.equal((await owner.post(`/api/dsp/documents/items/${safety.id}/trash`, {})).status, 200);
  assert.deepEqual(
    (await owner.get('/api/dsp/documents/folder')).value.items.map(
      (item: { name: string }) => item.name,
    ),
    ['Fuel cards'],
  );
});

type Person = { name: string; email: string; state: string; emailedAt: string | null };

/** Gives or takes Use Documents from the Member role of the DSP `dsp`, open in `owner`. */
async function useDocuments(owner: Client, dsp: string, on: boolean) {
  const roles: { id: string; name: string; permissions: string[] }[] = (
    await owner.get('/api/dsp/roles')
  ).value;
  const role = roles.find((each) => each.name === 'Member')!;
  const permissions = role.permissions.filter((p) => p !== 'documents.use');
  const saved = await owner.post(`/api/dsp/roles/${role.id}`, {
    name: role.name,
    permissions: on ? [...permissions, 'documents.use'] : permissions,
  });
  assert.equal(saved.status, 200, saved.body);
  // A role change expires the DSP view open on it.
  await owner.select(dsp);
}

/** The emails Dispatch sent, as fixture mode captures them. */
function sent(root: string): { to: string; subject: string; html: string }[] {
  const folder = path.join(root, 'data/platform/development-mail');
  if (!fs.existsSync(folder)) return [];
  return fs
    .readdirSync(folder)
    .filter((name) => name.endsWith('.json'))
    .map((name) => JSON.parse(fs.readFileSync(path.join(folder, name), 'utf8')));
}

test('the folder is shared with the team, and those without Google are emailed to link it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  // Fixture mode's Google knows no account at example.net.
  f.database('data/platform/accounts.sqlite', (db) =>
    db.prepare("UPDATE users SET email='jordan@example.net' WHERE email=?").run(demo.member),
  );
  const owner = await f.client();
  const member = await f.client('jordan@example.net');
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await useDocuments(owner, north.id, true);
  await member.select(north.id);
  await connect(owner);

  const team = async () => {
    const answer = await owner.get('/api/dsp/documents/team');
    assert.equal(answer.status, 200, answer.body);
    return answer.value as { people: Person[]; outsiders: unknown[]; storageUsed: number };
  };
  const people = (await team()).people;
  assert.deepEqual(
    people.map((person) => [person.name, person.email, person.state]),
    [['Jordan Ellis', 'jordan@example.net', 'needs_account']],
  );
  assert.ok(people[0]!.emailedAt);
  const link = () => sent(f.root).filter((mail) => mail.to === 'jordan@example.net');
  await until(async () => link().length === 1);
  assert.equal(
    link()[0]!.subject,
    '[Dispatch Dev] Link a Google account to edit Documents at Northline Logistics',
  );
  assert.match(link()[0]!.html, />Open Documents<\/a>/);
  // Opening the team again shares nothing new, and emails no one twice.
  await team();
  assert.equal(link().length, 1);
  assert.equal((await owner.read('/api/dsp/documents')).editors, 0);

  // Those who manage Documents can email again; members see only their own sharing.
  const again = await owner.post('/api/dsp/documents/team/email', {
    user: member.session.user.id,
  });
  assert.equal(again.status, 200, again.body);
  await until(async () => link().length === 2);
  // Only someone waiting on a Google account is emailed.
  assert.equal(
    (await owner.post('/api/dsp/documents/team/email', { user: owner.session.user.id })).value
      .error,
    'documents_person_not_found',
  );
  assert.equal((await member.get('/api/dsp/documents/team')).status, 403);
  assert.equal(
    (await member.post('/api/dsp/documents/team/email', { user: member.session.user.id })).status,
    403,
  );
  assert.equal((await member.post('/api/dsp/documents/team/remove', { share: 'any' })).status, 403);

  // The member links a Google account, and the folder is shared with it.
  const mine = (await member.read('/api/dsp/documents')).me;
  assert.deepEqual(mine, { state: 'needs_account', email: 'jordan@example.net', linked: false });
  const started = await member.post('/api/dsp/documents/link');
  assert.equal(started.status, 200, started.body);
  const { state, code } = signIn(started.value.url);
  assert.match(state, /\.link\./);
  // A link sign-in can't connect the DSP, nor a connect sign-in link an account.
  assert.equal(
    (await owner.post('/api/dsp/documents/connect/finish', { state, code })).value.error,
    'documents_connect_expired',
  );
  const connecting = signIn((await owner.post('/api/dsp/documents/connect')).value.url);
  assert.equal(
    (await member.post('/api/dsp/documents/link/finish', connecting)).value.error,
    'documents_connect_expired',
  );
  const linked = await member.post('/api/dsp/documents/link/finish', { state, code });
  assert.equal(linked.status, 200, linked.body);
  assert.deepEqual(linked.value, {
    state: 'shared',
    email: 'teammate@example.com',
    linked: true,
  });
  assert.deepEqual(
    (await team()).people.map((person) => [person.name, person.email, person.state]),
    [['Jordan Ellis', 'teammate@example.com', 'shared']],
  );
  assert.equal((await owner.read('/api/dsp/documents')).editors, 1);
  const events = (await owner.get('/api/platform/audit')).value;
  assert.ok(JSON.stringify(events).includes('documents.linked'));
});

test('members who stop using Documents lose the folder, and others shared in Google are listed', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await useDocuments(owner, north.id, true);
  await connect(owner);
  const team = async () =>
    (await owner.get('/api/dsp/documents/team')).value as {
      people: Person[];
      outsiders: { shareId: string; email: string }[];
    };
  assert.deepEqual(
    (await team()).people.map((person) => [person.email, person.state]),
    [['member@dispatch.test', 'shared']],
  );

  // Their share goes with the permission.
  await useDocuments(owner, north.id, false);
  const after = await team();
  assert.deepEqual(after.people, []);
  assert.deepEqual(after.outsiders, []);

  // A share Dispatch didn't make, as someone sharing the folder in Google Drive leaves.
  await useDocuments(owner, north.id, true);
  await team();
  f.database(`dsps/${north.id}/data/dispatch.sqlite`, (db) =>
    db.prepare("DELETE FROM documents_people WHERE shared_email='member@dispatch.test'").run(),
  );
  await useDocuments(owner, north.id, false);
  const outsiders = (await team()).outsiders;
  assert.deepEqual(
    outsiders.map((outsider) => outsider.email),
    ['member@dispatch.test'],
  );
  const removed = await owner.post('/api/dsp/documents/team/remove', {
    share: outsiders[0]!.shareId,
  });
  assert.equal(removed.status, 200, removed.body);
  assert.deepEqual(removed.value.outsiders, []);
  assert.equal(
    (await owner.post('/api/dsp/documents/team/remove', { share: outsiders[0]!.shareId })).value
      .error,
    'documents_share_not_found',
  );
});
