import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { demo, fixture, prepare, until } from '../../../../core/shell/tests/support/support.js';
import { capturedMail } from '../../../../core/shell/tests/support/mail-support.js';

type Client = Awaited<ReturnType<Awaited<ReturnType<typeof fixture>>['client']>>;

// Fixture mode stands in for Google: its sign-in sends the browser straight back with a
// code naming the account, `fixture:<email>`.
const signIn = (url: string) => {
  const sent = new URL(url).searchParams;
  return { state: sent.get('state')!, code: sent.get('code')! };
};
/** Where Google's return sends the browser for a sign-in: the page that finishes it. */
const returnedTo = async (url: string) => {
  const answer = await fetch(url, { redirect: 'manual' });
  const [page, query] = new URL(answer.headers.get('location')!).hash.split('?');
  const kept = new URLSearchParams(query);
  for (const field of ['googleState', 'googleCode', 'googleError']) kept.delete(field);
  return kept.size ? `${page}?${kept}` : page;
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
  // It's one of the DSP's own accounts: Google sends the browser back to its Connections.
  assert.equal(await returnedTo(started.value.url), `#dsp/${north.id}/settings?tab=connections`);
  // A sign-in belongs to the DSP it started in.
  await owner.select(summit.id);
  const elsewhere = await owner.post('/api/dsp/documents/connect/finish', { state, code });
  assert.equal(elsewhere.value.error, 'documents_connect_expired');
  await owner.select(north.id);
  const finished = await owner.post('/api/dsp/documents/connect/finish', { state, code });
  assert.equal(finished.status, 200, finished.body);
  const account = finished.value.account;
  assert.equal(account.connection.status, 'connected');
  assert.equal(account.connection.accountEmail, 'documents@example.com');
  assert.equal(account.connection.accountKind, 'personal');
  assert.equal(account.connection.folderName, 'Northline Logistics Documents');
  assert.equal(finished.value.madeFolder, true);
  assert.equal(account.storage.limit, 15_000_000_000);

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
  // Reconnecting the same account keeps its folder: nothing new to start with.
  const same = signIn((await owner.post('/api/dsp/documents/connect')).value.url);
  assert.equal(
    (await owner.post('/api/dsp/documents/connect/finish', same)).value.madeFolder,
    false,
  );

  const disconnected = await owner.post('/api/dsp/documents/disconnect');
  assert.equal(disconnected.status, 200, disconnected.body);
  assert.equal(disconnected.value.connection, null);
});

test('a role given Documents uses it, and only those who manage the DSP connections connect Google', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  // Every permission starts off, so the owner turns Documents on for the member's role.
  await useDocuments(owner, north.id, true);
  await member.select(north.id);
  const overview = await member.read('/api/dsp/documents');
  assert.equal(overview.connection, null);
  // Documents hasn't asked Google about them yet.
  assert.deepEqual(overview.me, { state: 'pending', email: demo.member, linked: false });
  // The platform owner isn't one of the DSP's members: the folder is never shared with them.
  assert.equal((await owner.read('/api/dsp/documents')).me, null);
  for (const [path, body] of [
    ['/api/dsp/documents/account', undefined],
    ['/api/dsp/documents/connect', {}],
    ['/api/dsp/documents/disconnect', {}],
  ] as const) {
    const refused = body ? await member.post(path, body) : await member.get(path);
    assert.equal(refused.status, 403, path);
  }
  // Every member adds from Drive: here only nothing's connected yet.
  assert.equal(
    (await member.post('/api/dsp/documents/add', { files: ['any'], folder: null })).value.error,
    'documents_not_connected',
  );
  const account = await owner.get('/api/dsp/documents/account');
  assert.equal(account.status, 200, account.body);
  assert.deepEqual(account.value, { connection: null, available: true, storage: null });
});

// Documents had a second permission, to manage it, which included using it. Its Google account
// is now one of the DSP's own, so a role kept from before uses Documents.
test('a role that managed Documents uses it after the update', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const north = (await f.client(demo.member)).session.dsps[0];
  await f.stop();
  f.people(north.id, (db) =>
    db.exec(
      `UPDATE roles SET permissions='["timecard.view","documents.manage"]' WHERE name='Member'`,
    ),
  );
  await f.start();
  const member = await f.client(demo.member);
  assert.deepEqual([...(await member.select(north.id)).permissions].sort(), [
    'documents.use',
    'timecard.view',
  ]);
});

/** Connects the DSP open in `client` to fixture mode's Google account. */
async function connect(client: Client) {
  const { state, code } = signIn((await client.post('/api/dsp/documents/connect')).value.url);
  const finished = await client.post('/api/dsp/documents/connect/finish', { state, code });
  assert.equal(finished.status, 200, finished.body);
}

test('a team makes, finds, renames, moves and trashes files, and never reaches another DSP’s', async (t) => {
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

  // A file moves to the top and back, each answer naming the folder it left; a folder takes
  // what it holds with it, but never goes inside itself or a folder inside it.
  const move = (id: string, folder: string | null) =>
    owner.post(`/api/dsp/documents/items/${id}/move`, { folder });
  const listed = async (folder?: string) =>
    (await owner.get(`/api/dsp/documents/folder${folder ? `?id=${folder}` : ''}`)).value;
  const out = await move(plan.id, null);
  assert.equal(out.value.from, safety.id, out.body);
  const atTop = (await listed()).items as { name: string; items: number | null }[];
  assert.equal(atTop.find((item) => item.name === 'Safety & Compliance')?.items, 0);
  assert.ok(atTop.some((item) => item.name === 'Rescue plan October'));
  assert.equal((await move(plan.id, safety.id)).value.from, null);
  const audits = await make('folder', 'Audits', safety.id);
  for (const into of [safety.id, audits.id])
    assert.equal((await move(safety.id, into)).value.error, 'documents_move_inside');
  const archive = await make('folder', 'Archive');
  assert.equal((await move(safety.id, archive.id)).status, 200);
  const archived = await listed(safety.id);
  assert.deepEqual(archived.path, [
    { id: archive.id, name: 'Archive' },
    { id: safety.id, name: 'Safety & Compliance' },
  ]);
  assert.deepEqual(archived.items.map((item: { name: string }) => item.name).sort(), [
    'Audits',
    'Rescue plan October',
  ]);
  assert.equal((await move(safety.id, null)).value.from, archive.id);

  // Summit connects the same Google account, and none of Northline's files are its to reach.
  await owner.select(summit.id);
  await connect(owner);
  assert.deepEqual((await owner.get('/api/dsp/documents/folder')).value.items, []);
  for (const [path, body] of [
    [`/api/dsp/documents/folder?id=${safety.id}`, undefined],
    [`/api/dsp/documents/items/${plan.id}/rename`, { name: 'Mine now' }],
    [`/api/dsp/documents/items/${plan.id}/trash`, {}],
    [`/api/dsp/documents/items/${plan.id}/move`, { folder: null }],
    ['/api/dsp/documents/new', { folder: safety.id, kind: 'doc', name: 'Mine now' }],
  ] as const) {
    const refused = body ? await owner.post(path, body) : await owner.get(path);
    assert.equal(refused.value.error, 'documents_item_not_found', path);
  }
  // Nor does Summit's own file go into one of Northline's folders.
  const notes = await make('doc', 'Summit notes');
  assert.equal((await move(notes.id, safety.id)).value.error, 'documents_item_not_found');

  await owner.select(north.id);
  assert.equal((await owner.post(`/api/dsp/documents/items/${safety.id}/trash`, {})).status, 200);
  assert.deepEqual(
    (await owner.get('/api/dsp/documents/folder')).value.items.map(
      (item: { name: string }) => item.name,
    ),
    ['Archive', 'Fuel cards'],
  );
});

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

test('the folder is shared with the team, and those without Google are emailed to link one', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await useDocuments(owner, north.id, true);
  // Riley joins the team at an address fixture mode's Google knows no account at.
  const roles: { id: string; name: string }[] = (await owner.get('/api/dsp/roles')).value;
  const invited = await owner.post('/api/dsp/members/invite', {
    email: 'riley@example.net',
    role: roles.find((role) => role.name === 'Member')!.id,
  });
  assert.equal(invited.status, 200, invited.body);
  const token = /token=([A-Za-z0-9_-]{43})/.exec(
    (await capturedMail(f.root, 'riley@example.net')).text,
  )![1];
  const accepted = await f.request(
    `/api/invitations/${token}/accept`,
    { firstName: 'Riley', lastName: 'Park', password: demo.password },
    f.at(north.code),
  );
  assert.equal(accepted.status, 200, JSON.stringify(accepted.value));
  const member = await f.client('riley@example.net', demo.password, north.code);
  await member.select(north.id);
  const jordan = await f.client(demo.member);
  await jordan.select(north.id);
  await connect(owner);

  // Each sees how the folder is shared with them, once Documents has asked Google.
  const mine = async (client: Client) => (await client.read('/api/dsp/documents')).me;
  await until(async () => (await mine(jordan)).state === 'shared');
  await until(async () => (await mine(member)).state === 'needs_account');
  assert.deepEqual(await mine(member), {
    state: 'needs_account',
    email: 'riley@example.net',
    linked: false,
  });
  const link = () =>
    sent(f.root).filter(
      (mail) => mail.to === 'riley@example.net' && mail.subject.includes('Link a Google account'),
    );
  await until(async () => link().length === 1);
  assert.equal(
    link()[0]!.subject,
    '[Dispatch Dev] Link a Google account to edit Documents at Northline Logistics',
  );
  assert.match(link()[0]!.html, />Open Documents<\/a>/);

  // The member links a Google account, and the folder is shared with it. Started on Documents,
  // Google sends them back there; on Settings' Connections, back there.
  const fromSettings = await member.post('/api/dsp/documents/link', { from: 'settings' });
  assert.equal(
    await returnedTo(fromSettings.value.url),
    `#dsp/${north.id}/settings?tab=connections`,
  );
  const started = await member.post('/api/dsp/documents/link', {});
  assert.equal(started.status, 200, started.body);
  assert.equal(await returnedTo(started.value.url), `#dsp/${north.id}/documents`);
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
  assert.deepEqual(await mine(member), linked.value);
  const events = (await owner.get('/api/platform/audit')).value;
  assert.ok(JSON.stringify(events).includes('documents.linked'));
});

test('members who stop using Documents lose the folder', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await useDocuments(owner, north.id, true);
  await member.select(north.id);
  await connect(owner);
  await until(async () => (await member.read('/api/dsp/documents')).me.state === 'shared');
  const shared = () =>
    f.database(`dsps/${north.id}/data/dispatch.sqlite`, (db) =>
      db.prepare('SELECT shared_email FROM documents_people').all(),
    ) as { shared_email: string }[];
  assert.deepEqual(
    shared().map((row) => row.shared_email),
    [demo.member],
  );

  // Their share goes with the permission, as soon as Documents shares the folder again: within
  // the minute, or here, as reconnecting does.
  await useDocuments(owner, north.id, false);
  await connect(owner);
  await until(async () => shared().length === 0);
});

test('a team uploads files into its folders, sees pictures of them, renames them keeping their extensions, and downloads them and Google’s own as Office files', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsps = owner.session.dsps as { id: string; name: string }[];
  const north = dsps.find((d) => d.name === 'Northline Logistics')!;
  const summit = dsps.find((d) => d.name === 'Summit Delivery')!;
  await owner.select(north.id);
  await connect(owner);
  const safety = (
    await owner.post('/api/dsp/documents/new', {
      folder: null,
      kind: 'folder',
      name: 'Safety',
    })
  ).value;
  const origin = f.env.DISPATCH_ORIGIN!;
  // The browser sends a file as its own body, its name and type in the address.
  const send = (path: string, body: BodyInit, headers: Record<string, string> = {}) =>
    fetch(origin + path, {
      method: 'POST',
      headers: {
        ...owner.headers,
        origin,
        'content-type': 'application/octet-stream',
        ...headers,
      },
      body,
    });
  const into = (folder: string | null, name: string, type?: string) =>
    `/api/dsp/documents/upload?${new URLSearchParams({
      ...(folder ? { folder } : {}),
      name,
      ...(type ? { type } : {}),
    })}`;
  const pdf = '%PDF-1.4 fixture rescue plan';
  const uploaded = await send(into(safety.id, 'Rescue plan.pdf', 'application/pdf'), pdf);
  assert.equal(uploaded.status, 200, await uploaded.clone().text());
  const file = await uploaded.json();
  assert.deepEqual(
    [file.name, file.kind, file.size, file.addedBy],
    ['Rescue plan.pdf', 'pdf', pdf.length, 'Platform support'],
  );
  const inside = (await owner.get(`/api/dsp/documents/folder?id=${safety.id}`)).value.items;
  assert.deepEqual(
    inside.map((item: { name: string; size: number | null }) => [item.name, item.size]),
    [['Rescue plan.pdf', pdf.length]],
  );

  const download = (id: string) =>
    fetch(`${origin}/api/dsp/documents/items/${id}/download`, { headers: owner.headers });
  const saved = await download(file.id);
  assert.equal(saved.status, 200);
  assert.equal(saved.headers.get('content-type'), 'application/pdf');
  assert.match(saved.headers.get('content-disposition')!, /filename="Rescue plan\.pdf"/);
  assert.equal(await saved.text(), pdf);
  // Google's own come down as the Office files they export as.
  const doc = (
    await owner.post('/api/dsp/documents/new', { folder: null, kind: 'doc', name: 'Plan' })
  ).value;
  const exported = await download(doc.id);
  assert.equal(
    exported.headers.get('content-type'),
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  );
  assert.match(exported.headers.get('content-disposition')!, /filename="Plan\.docx"/);
  assert.equal((await (await download(safety.id)).json()).error, 'documents_item_not_found');

  // An uploaded file keeps its extension when renamed; Google's own have none to keep.
  assert.equal(file.extension, '.pdf');
  const rename = (id: string, name: string) =>
    owner.post(`/api/dsp/documents/items/${id}/rename`, { name });
  for (const name of ['Rescue plan.exe', 'Rescue plan', 'Rescue plan.PDF', '.pdf'])
    assert.equal((await rename(file.id, name)).value.error, 'documents_extension_kept', name);
  assert.equal((await rename(file.id, 'Rescue plan v2.1.pdf')).value.name, 'Rescue plan v2.1.pdf');
  assert.equal(doc.extension, null);
  assert.equal((await rename(doc.id, 'Plan v2.1')).value.name, 'Plan v2.1');

  // An image's card shows Google's picture of it, which the browser keeps for that view.
  const png = 'fixture png bytes';
  const photo = await (await send(into(null, 'Route map.png', 'image/png'), png)).json();
  const listed = (await owner.get('/api/dsp/documents/folder')).value.items.find(
    (item: { id: string }) => item.id === photo.id,
  );
  const picture = (url: string) => fetch(origin + url, { headers: owner.headers });
  const pictured = await picture(listed.thumbnail);
  assert.equal(pictured.status, 200);
  assert.equal(pictured.headers.get('content-type'), 'image/png');
  assert.match(pictured.headers.get('cache-control')!, /^private, max-age=\d+/);
  assert.equal(pictured.headers.get('vary'), 'X-Dispatch-View');
  assert.equal(await pictured.text(), png);

  // A type naming one of Google's own is only bytes, and a name is checked as any is.
  const typed = await (
    await send(into(null, 'notes', 'application/vnd.google-apps.document'), 'hi')
  ).json();
  assert.equal(typed.kind, 'file');
  assert.equal((await (await send(into(null, ' '), 'hi')).json()).error, 'documents_name_invalid');
  // A JSON body is no upload, and one too large is refused before a byte is read.
  assert.equal(
    (await (await send(into(null, 'notes'), '{}', { 'content-type': 'application/json' })).json())
      .error,
    'upload_required',
  );
  const tooLarge = await new Promise<{ status: number; body: string }>((resolve, reject) => {
    const request = http.request(origin + into(null, 'huge.bin'), {
      method: 'POST',
      headers: {
        ...owner.headers,
        origin,
        'content-type': 'application/octet-stream',
        'content-length': String(101 * 1024 * 1024),
      },
    });
    request.on('response', (response) => {
      let body = '';
      response.on('data', (chunk) => (body += chunk));
      response.on('end', () => {
        request.destroy();
        resolve({ status: response.statusCode!, body });
      });
    });
    request.on('error', reject);
    request.write('a');
  });
  assert.equal(tooLarge.status, 413);
  assert.equal(JSON.parse(tooLarge.body).error, 'upload_too_large');

  // Another DSP connecting the same account reaches none of it.
  await owner.select(summit.id);
  await connect(owner);
  assert.equal(
    (await (await send(into(safety.id, 'Mine.pdf'), pdf)).json()).error,
    'documents_item_not_found',
  );
  assert.equal((await (await download(file.id)).json()).error, 'documents_item_not_found');
  assert.equal((await (await picture(listed.thumbnail)).json()).error, 'documents_item_not_found');
});

test('every member adds files and folders from Drive, and what the team makes in the folder shows', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await useDocuments(owner, north.id, true);
  await member.select(north.id);
  await connect(owner);
  type Picked = { id: string; name: string; kind: string };
  const setup = async () => (await owner.read('/api/dsp/documents')).picker;
  // Fixture mode has no picker of Google's: it names what its Drive holds outside Documents.
  const picker = await setup();
  assert.equal(picker.account, 'documents@example.com');
  assert.equal(picker.google, null);
  const made = (name: string) => picker.elsewhere.find((file: Picked) => file.name === name)!;
  assert.deepEqual(picker.elsewhere.map((file: Picked) => [file.name, file.kind]).sort(), [
    ['Fuel prices', 'sheet'],
    ['Route map 2025.png', 'image'],
    ['Route tracker', 'sheet'],
  ]);
  const listed = async (folder?: string) =>
    (await owner.read(`/api/dsp/documents/folder${folder ? `?id=${folder}` : ''}`)).items as {
      id: string;
      name: string;
    }[];
  const top = async () => (await listed()).map((item) => item.name);
  const named = async (name: string, folder?: string) =>
    (await listed(folder)).find((item) => item.name === name)!;
  // Dispatch reaches the whole Drive: what the team made directly in the folder meanwhile,
  // as fixture mode's does by the time the DSP reconnects, shows without being added, folders
  // and all.
  assert.deepEqual(await top(), []);
  await connect(owner);
  assert.deepEqual(await top(), ['Safety', 'Fuel receipts.pdf']);
  const safetyFolder = await named('Safety');
  assert.deepEqual(
    (await listed(safetyFolder.id)).map((item) => item.name),
    ['Weekly safety huddle'],
  );

  // One already inside is only recorded as added; one elsewhere in the Drive moves in.
  const fleet = (
    await owner.post('/api/dsp/documents/new', { folder: null, kind: 'folder', name: 'Fleet' })
  ).value;
  const receipts = await named('Fuel receipts.pdf');
  const added = await owner.post('/api/dsp/documents/add', {
    files: [receipts.id, made('Route map 2025.png').id],
    folder: fleet.id,
  });
  assert.equal(added.status, 200, added.body);
  assert.deepEqual(
    added.value.items.map((item: { name: string; addedBy: string }) => [item.name, item.addedBy]),
    [
      ['Fuel receipts.pdf', 'Platform support'],
      ['Route map 2025.png', 'Platform support'],
    ],
  );
  assert.deepEqual(await top(), ['Fleet', 'Safety', 'Fuel receipts.pdf']);
  assert.deepEqual(
    (await listed(fleet.id)).map((item) => item.name),
    ['Route map 2025.png'],
  );
  // One someone else owns and shared with the account stays where its owner keeps it:
  // Documents gets a shortcut to it, which shows and opens as the Sheet, once however often
  // it's added.
  const shared = made('Route tracker');
  for (const _ of [1, 2]) {
    const tracker = await owner.post('/api/dsp/documents/add', {
      files: [shared.id],
      folder: null,
    });
    assert.equal(tracker.status, 200, tracker.body);
    assert.deepEqual(tracker.value.unshared, []);
  }
  const trackers = (await owner.read('/api/dsp/documents/folder')).items.filter(
    (item: { name: string }) => item.name === 'Route tracker',
  );
  assert.equal(trackers.length, 1);
  assert.notEqual(trackers[0].id, shared.id);
  assert.equal(trackers[0].kind, 'sheet');
  assert.equal(trackers[0].url, `https://docs.google.com/spreadsheets/d/${shared.id}/edit`);
  assert.equal(trackers[0].shortcut, true);
  assert.equal(trackers[0].unavailable, false);
  // The team gets the Sheet shared with them, where the folder is shared with them.
  const dsp = <T>(query: string) =>
    f.database(`dsps/${north.id}/data/dispatch.sqlite`, (db) => db.prepare(query).all()) as T[];
  const sharedFiles = () =>
    dsp<{ file_id: string; email: string }>('SELECT file_id, email FROM documents_shared_files');
  const team = dsp<{ shared_email: string }>(
    "SELECT shared_email FROM documents_people WHERE state='shared'",
  ).map((person) => person.shared_email);
  assert.ok(team.length > 0);
  await until(async () => sharedFiles().length === team.length);
  assert.deepEqual(
    sharedFiles()
      .map((share) => [share.file_id, share.email])
      .sort(),
    team.map((email) => [shared.id, email]).sort(),
  );
  // One the account may only view, it can't share on: the team asks its owner for it.
  const prices = await owner.post('/api/dsp/documents/add', {
    files: [made('Fuel prices').id],
    folder: null,
  });
  assert.equal(prices.status, 200, prices.body);
  assert.deepEqual(prices.value.unshared, ['Fuel prices']);
  assert.equal(prices.value.items[0].shortcut, true);
  assert.deepEqual((await setup()).elsewhere, []);
  // Picking the main folder itself changes nothing.
  const main = await owner.post('/api/dsp/documents/add', { files: [picker.folder], folder: null });
  assert.equal(main.status, 200, main.body);
  assert.deepEqual(main.value, { items: [], unshared: [] });
  // Shortcuts sort as their files changed, so only what's at the top is checked here.
  assert.deepEqual((await top()).sort(), [
    'Fleet',
    'Fuel prices',
    'Fuel receipts.pdf',
    'Route tracker',
    'Safety',
  ]);
  // Taking the shortcut out of Documents takes back the shares Dispatch made of the Sheet.
  const trashed = await owner.post(`/api/dsp/documents/items/${trackers[0].id}/trash`, {});
  assert.equal(trashed.status, 200, trashed.body);
  await until(async () => sharedFiles().length === 0);
  // A file the account that holds Documents doesn't reach isn't added.
  assert.equal(
    (await owner.post('/api/dsp/documents/add', { files: ['someone-elses-file'], folder: null }))
      .value.error,
    'documents_picked_elsewhere',
  );
  assert.equal(
    (await owner.post('/api/dsp/documents/add', { files: [], folder: null })).value.error,
    'invalid_input',
  );

  // Every member adds from Drive, picking as themselves where the folder is shared with them.
  await until(async () => (await member.read('/api/dsp/documents')).me?.state === 'shared');
  const theirs = (await member.read('/api/dsp/documents')).picker;
  assert.equal(theirs.me, demo.member);
  assert.equal(theirs.account, 'documents@example.com');
  const byMember = await member.post('/api/dsp/documents/add', {
    files: [receipts.id],
    folder: null,
  });
  assert.equal(byMember.status, 200, byMember.body);
});

test("Google's picker runs on a page of its own, and the dashboard loads none of Google", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const picker = await f.raw('/api/documents/google/picker');
  assert.equal(picker.status, 200);
  assert.match(picker.headers.get('content-type')!, /^text\/html/);
  const policy = picker.headers.get('content-security-policy')!;
  assert.match(policy, /script-src 'nonce-[\w-]+' https:\/\/apis\.google\.com;/);
  assert.match(policy, /connect-src https:\/\/www\.googleapis\.com;/);
  assert.match(policy, /frame-ancestors 'none'/);
  assert.equal(picker.headers.get('cross-origin-opener-policy'), 'same-origin-allow-popups');
  const nonce = /'nonce-([\w-]+)'/.exec(policy)![1];
  const page = await picker.text();
  assert.ok(page.includes(`<script nonce="${nonce}">`));
  // Google's picker key works only from Dispatch's own address, which the page names to it.
  assert.ok(page.includes('<meta name="referrer" content="strict-origin">'));
  // Google sends the browser back to the platform's own return address, the one it knows,
  // and the page names the DSP address it's at, to go back on to.
  const back = `${f.env.DISPATCH_ORIGIN}/api/documents/google/return`;
  assert.ok(page.includes(JSON.stringify({ back, code: '' })));
  // At a DSP's own address, which the test's requests reach by its Host.
  const atDsp = await new Promise<{ status: number; body: string }>((resolve, reject) => {
    http
      .get(
        `${f.env.DISPATCH_ORIGIN}/api/documents/google/picker`,
        { headers: f.at('nll') },
        (res) => {
          let body = '';
          res.on('data', (chunk) => (body += chunk));
          res.on('end', () => resolve({ status: res.statusCode!, body }));
        },
      )
      .on('error', reject);
  });
  assert.equal(atDsp.status, 200);
  assert.ok(atDsp.body.includes(JSON.stringify({ back, code: 'nll' })));
  const dashboard = await f.raw('/');
  assert.doesNotMatch(dashboard.headers.get('content-security-policy')!, /google/);
  assert.equal(dashboard.headers.get('cross-origin-opener-policy'), 'same-origin');
});

test("Google's return address takes the picker's answer on, in the fragment, to the picker's page", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const relay = await f.raw('/api/documents/google/return');
  assert.equal(relay.status, 200);
  const policy = relay.headers.get('content-security-policy')!;
  assert.match(policy, /^default-src 'none'; script-src 'nonce-[\w-]+'; base-uri 'none'/);
  const page = await relay.text();
  assert.ok(
    page.includes(
      JSON.stringify({
        origin: f.env.DISPATCH_ORIGIN,
        dsp: `http://{code}.localhost:${f.env.PORT}`,
        picker: '/api/documents/google/picker',
      }),
    ),
  );
});

test("a server with half of Google's settings refuses to start, naming what's missing", async (t) => {
  const f = await prepare();
  t.after(f.cleanup);
  const secret = 'private-client-secret-value';
  const result = spawnSync(f.binary, ['serve'], {
    env: { ...f.env, DISPATCH_DEV_GOOGLE_CLIENT_SECRET: secret },
    encoding: 'utf8',
    timeout: 30000,
  });
  const output = `${result.stdout}${result.stderr}`;
  assert.notEqual(result.status, 0, output);
  assert.match(output, /feature_settings_incomplete/);
  assert.match(output, /DISPATCH_DEV_GOOGLE_CLIENT_ID/);
  assert.ok(!output.includes(secret));
});
