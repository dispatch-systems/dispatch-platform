import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';

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
async function connect(client: Awaited<ReturnType<Awaited<ReturnType<typeof fixture>>['client']>>) {
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
