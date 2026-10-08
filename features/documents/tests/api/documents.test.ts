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

test('everyone on the team sees Documents, and only those who manage it connect Google', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const member = await f.client(demo.member);
  const north = member.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await member.select(north.id);
  const overview = await member.read('/api/dsp/documents');
  assert.equal(overview.connection, null);
  assert.ok(overview.owners.length > 0);
  assert.equal((await member.post('/api/dsp/documents/connect')).status, 403);
  assert.equal((await member.post('/api/dsp/documents/disconnect')).status, 403);
});
