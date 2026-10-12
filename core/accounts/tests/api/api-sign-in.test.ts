import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { demo, fixture, until } from '../../../shell/tests/support/support.js';
const { password } = demo;

test('Rust password recovery uses the private outbox, revokes sessions and consumes reset links once', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  f.database('data/platform/accounts.sqlite', (db) =>
    db
      .prepare('INSERT INTO passkeys VALUES (?,?,?,?,?)')
      .run('legacy-owner', owner.session.user.id, '{}', 'Old key', 0),
  );
  assert.equal(
    (await f.request('/api/auth/forgot-password', { email: 'owner@dispatch.test' })).status,
    202,
  );
  const mail = path.join(f.root, 'data/platform/development-mail');
  const completed = () =>
    fs.existsSync(mail) ? fs.readdirSync(mail).filter((name) => name.endsWith('.json')) : [];
  await until(async () => completed().length > 0);
  const filename = path.join(mail, completed()[0]!);
  assert.equal(fs.statSync(filename).mode & 0o077, 0);
  const message = JSON.parse(fs.readFileSync(filename, 'utf8'));
  assert.match(message.html, />Reset password<\/a>/);
  const raw = /token=([A-Za-z0-9_-]{43})/.exec(message.text)![1];
  assert.equal(
    (await f.request('/api/auth/reset-password', { token: raw, password: 'Replacement-password!' }))
      .status,
    200,
  );
  assert.equal((await owner.get('/api/session')).status, 401);
  assert.equal((await f.request('/api/auth/reset-password', { token: raw, password })).status, 400);
  const renewed = await f.client('owner@dispatch.test', 'Replacement-password!');
  assert.equal((await renewed.get('/api/platform/dsps')).status, 200);
  assert.equal(
    (await renewed.post('/api/auth/password', { currentPassword: 'wrong', password })).status,
    403,
  );
  assert.equal(
    (
      await renewed.post('/api/auth/password', {
        currentPassword: 'Replacement-password!',
        password,
      })
    ).status,
    200,
  );
  assert.equal((await renewed.get('/api/session')).status, 401);
});

test('password recovery gives existing and absent accounts the same delayed public answer', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const answers: { status: number; value: unknown; elapsed: number }[] = [];
  for (const email of ['owner@dispatch.test', 'absent@dispatch.test']) {
    const started = performance.now();
    const answer = await f.request('/api/auth/forgot-password', { email });
    answers.push({
      status: answer.status,
      value: answer.value,
      elapsed: performance.now() - started,
    });
  }
  for (const answer of answers) {
    assert.equal(answer.status, 202);
    assert.deepEqual(answer.value, { ok: true });
    assert(answer.elapsed >= 280, `recovery response returned in ${answer.elapsed}ms`);
  }
});

test('each address signs in only its own people, and a session works only where it was made', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const north = f.at(demo.memberDsp);
  const login = async (email: string, at: Record<string, string>) => {
    const answer = await f.request('/api/auth/login', { email, password }, at);
    return [answer.status, answer.value.error];
  };
  // The platform owner signs in at the admin's address alone, a member at their DSP's alone;
  // anywhere else, a right password reads as a wrong one.
  const refused = [401, 'invalid_login'];
  assert.deepEqual(await login(demo.member, {}), refused);
  assert.deepEqual(await login(demo.email, north), refused);
  assert.deepEqual(await login(demo.member, f.at('sum')), refused);
  assert.deepEqual(await login(demo.member, f.at('invite')), refused);
  assert.deepEqual(await login(demo.member, f.at('zzz')), refused);
  const site = async (at: Record<string, string>) =>
    (await f.request('/api/site', undefined, at)).value;
  assert.equal((await site({})).kind, 'admin');
  assert.equal((await site(f.at('invite'))).kind, 'invite');
  assert.deepEqual((await site(f.at('zzz'))).dsp, null);
  assert.equal((await f.request('/api/site', undefined, f.at('admin'))).status, 400);

  // At its DSP's address, a session lists and opens that DSP alone.
  const owner = await f.client();
  const summit = owner.session.dsps.find((d: { name: string }) => d.name === 'Summit Delivery');
  const joined = await f.client(demo.member);
  assert.deepEqual(
    joined.session.dsps.map((d: { code: string }) => d.code),
    [demo.memberDsp],
  );
  assert.equal((await site(north)).dsp.id, joined.session.dsps[0].id);
  const other = await joined.post('/api/session/dsp', { dspId: summit.id });
  assert.deepEqual([other.status, other.value.error], [403, 'permission_denied']);
  // Its cookie is no session at any other address, nor are the admin's at a DSP's.
  const elsewhere = { ...joined.headers, ...f.at('sum') };
  assert.equal((await f.request('/api/session', undefined, elsewhere)).status, 401);
  const admin = { cookie: joined.headers.cookie!, host: new URL(f.env.DISPATCH_ORIGIN!).host };
  assert.equal((await f.request('/api/session', undefined, admin)).status, 401);
  assert.equal(
    (await f.request('/api/session', undefined, { ...owner.headers, ...north })).status,
    401,
  );
  // A write is taken only from the pages of the address it comes to.
  const forged = await f.request('/api/auth/logout', {}, { ...joined.headers, origin: 'x' });
  assert.deepEqual([forged.status, forged.value.error], [403, 'invalid_origin']);
  // Agents reach Dispatch at the admin's address alone.
  assert.equal((await f.request('/api/v1/whoami', undefined, north)).status, 404);
});

// Protects the HTTP boundary that signed-claim unit tests cannot reach: cookies must stay
// read only on public/async routes and GET writes, even alongside a human sign-in. Uses
// the installed CLI and live route table, without a token-minting test-only API.
test('host-issued observation sign-in cannot mutate or fall back to a human session', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const canonical = { host: new URL(f.env.DISPATCH_ORIGIN!).host };
  const authorityRows = () =>
    f.database('data/platform/accounts.sqlite', (db) =>
      db
        .prepare(
          `SELECT (SELECT count(*) FROM sessions WHERE user_id=?) sessions,
      (SELECT count(*) FROM audit WHERE actor_id=? AND action IN ('account.signed_in','dsp.owner_view_opened')) audits`,
        )
        .get(owner.session.user.id, owner.session.user.id),
    );
  const before = authorityRows();
  const output = path.join(f.root, 'observation.json');
  const metadata = JSON.parse(f.cli(['observation-session', output]));
  const credential = JSON.parse(fs.readFileSync(output, 'utf8'));
  assert.equal(fs.statSync(output).mode & 0o777, 0o600);
  assert.equal(metadata.token, undefined);
  assert.equal(metadata.expiresAt, credential.expiresAt);
  const exchange = () => f.request('/api/auth/observation', { token: credential.token }, canonical);
  assert.equal(
    (await f.request('/api/auth/observation', { token: 'forged' }, canonical)).status,
    401,
  );
  assert.equal(
    (await f.request('/api/auth/observation', { token: credential.token }, f.at('nll'))).status,
    401,
  );
  const issued = await exchange();
  assert.equal(issued.status, 200, issued.body);
  assert.equal(issued.value.expiresAt, credential.expiresAt);
  const setCookie = issued.headers.get('set-cookie')!;
  assert.match(setCookie, /; HttpOnly; SameSite=Strict;/);
  const cookie = setCookie.split(';')[0]!;
  const observer = { ...canonical, cookie };
  const session = await f.request('/api/session', undefined, observer);
  assert.equal(session.status, 200, session.body);
  assert.equal(session.value.user.id, owner.session.user.id);
  assert.equal(session.value.readOnly, true);
  const headers = {
    ...observer,
    cookie: `${cookie}; ${owner.headers.cookie}`,
    'x-csrf-token': session.value.csrf,
  };
  const view = await f.request('/api/session/dsp', { dspId: owner.session.dsps[0].id }, headers);
  assert.equal(view.status, 200, view.body);
  assert.deepEqual(
    authorityRows(),
    before,
    'mint, exchange and observation views leave human session and visit records untouched',
  );
  const authorized = { ...headers, 'x-dispatch-view': view.value.token };
  assert.equal((await f.request('/api/platform/dsps', undefined, headers)).status, 200);
  for (const route of [
    '/api/auth/login',
    '/api/auth/logout',
    '/api/auth/forgot-password',
    '/api/auth/security/passkeys/register/start',
    '/api/platform/dsps',
    '/api/dsp/employees/E001/sync',
    '/oauth/token',
    '/api/v1/mcp',
  ]) {
    const denied = await f.request(route, {}, authorized);
    assert.equal(denied.status, 403, route);
    assert.equal(denied.value.error, 'observation_read_only', route);
  }
  assert.equal(
    (await f.request('/api/invitations/nonexistent', undefined, headers)).value.error,
    'observation_read_only',
  );
  for (const invalidCookie of [
    'dispatch_observation_read=',
    'dispatch_observation_read',
    'dispatch_observation_read=forged',
    `dispatch_observation_read=${owner.headers.cookie!.split('=')[1]}`,
    `${cookie}; ${cookie}`,
    `__Host-dispatch_observation_read=forged; ${cookie}`,
  ]) {
    const answer = await f.request('/api/session', undefined, {
      ...canonical,
      cookie: `${invalidCookie}; ${owner.headers.cookie}`,
    });
    assert.equal(answer.status, 401, invalidCookie.split('=')[0]);
  }
  assert.equal(
    (await f.request('/api/session', undefined, { cookie })).status,
    401,
    'loopback alias',
  );
  for (const name of ['dispatch_session', 'dispatch_support_read', 'unrelated_cookie']) {
    const misplaced = {
      ...canonical,
      cookie: `${name}=${credential.token}; ${owner.headers.cookie}`,
      'x-csrf-token': owner.session.csrf,
    };
    assert.equal((await f.request('/api/session', undefined, misplaced)).status, 401, name);
    assert.equal(
      (await f.request('/api/session/dsp', { dspId: owner.session.dsps[0].id }, misplaced)).status,
      401,
      name,
    );
    assert.equal(
      (await f.request('/api/invitations/nonexistent', undefined, misplaced)).value.error,
      'observation_read_only',
      name,
    );
  }
  const renewed = await f.request('/api/auth/observation', { token: credential.token }, headers);
  assert.equal(renewed.status, 200, renewed.body);
  assert.equal(renewed.value.expiresAt, credential.expiresAt, 'exchange must not extend lifetime');
  assert(!f.logs().includes(credential.token), 'tokens must not enter request logs');
  f.database('data/platform/accounts.sqlite', (db) =>
    db.prepare('UPDATE users SET version=version+1 WHERE id=?').run(owner.session.user.id),
  );
  assert.equal((await f.request('/api/session', undefined, headers)).status, 401);
  assert.equal((await exchange()).status, 401);
});
