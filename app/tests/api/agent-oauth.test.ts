import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fixture, freePort } from '../../../core/shell/tests/support/support.js';
import { capturedMail } from '../../../core/shell/tests/support/mail-support.js';
import type {
  AgentKey,
  AgentKeys,
  AgentWhoami,
  OAuthAllowedApps,
  OAuthApproval,
  OAuthPairing,
  OAuthPairingOpened,
  OAuthRedirect,
  OAuthRequest,
} from '../../../core/platform_owner/api/index.js';
import { everyKind } from '../support/agent-keys.js';

// Sign in with Dispatch, driven over HTTP the way an MCP client drives it: a 401 that says
// where to sign in, the discovery documents, the browser's trip through /oauth/authorize, the
// owner's approval on the dashboard's API, then form posts to the token and revocation
// endpoints, and the MCP endpoint with the access token. Apps ask only while the owner has
// the pairing window open, so each test opens it first unless it is about the window.

const claudeCode = 'https://claude.ai/oauth/claude-code-client-metadata';
const chatgpt = 'https://chatgpt.com/oauth/client.json';
const chatgptRedirect = 'https://chatgpt.com/connector_platform_oauth_redirect';
const cursorRedirect = 'cursor://anysphere.cursor-retrieval/oauth/callback';
// The tools an app reading routes and DVIC is offered, as agent-mcp.test.ts lists them for a
// key: theirs, and those that read nothing in particular.
const routesAndDvic: OAuthApproval['reads'] = { areas: ['routes', 'dvic'], bypass: false };
const routesAndDvicTools = [
  'data_status',
  'driver_report',
  'dvic_inspections',
  'find_drivers',
  'find_package',
  'get_profile',
  'list_metrics',
  'packages',
  'route_day',
  'route_stops',
  'team_table',
  'whoami',
];

type App = Awaited<ReturnType<typeof fixture>>;
type Answer = { status: number; headers: Headers; body: any };
type Tokens = {
  access_token: string;
  token_type: string;
  expires_in: number;
  refresh_token: string;
  scope: string;
};

const everything = (name: string): OAuthApproval => ({
  name,
  allDsps: true,
  dsps: [],
  reads: { areas: ['locations', ...everyKind], bypass: false },
});

/** A PKCE pair as a client makes one: a random verifier and its S256 challenge. */
function pkce() {
  const verifier = crypto.randomBytes(32).toString('base64url');
  const challenge = crypto.createHash('sha256').update(verifier).digest('base64url');
  return { verifier, challenge };
}
/** A redirect's query parameters, whatever its scheme. */
function params(location: string) {
  const at = location.indexOf('?');
  return new URLSearchParams(at < 0 ? '' : location.slice(at + 1));
}

/** The platform owner opens the pairing window, as copying a sign-in command does. */
async function openPairing(owner: Awaited<ReturnType<App['client']>>) {
  const opened = await owner.post('/api/platform/oauth/pairing');
  assert.equal(opened.status, 200, opened.body);
  return (opened.value as OAuthPairingOpened).openUntil;
}

/** The captured email to `to` with this subject, once the mailer has delivered it. */
async function mailed(root: string, to: string, subject: string) {
  const directory = path.join(root, 'data/platform/development-mail');
  const deadline = Date.now() + 12000;
  while (Date.now() < deadline) {
    for (const name of fs.existsSync(directory) ? fs.readdirSync(directory) : []) {
      if (!name.endsWith('.json')) continue;
      const message = JSON.parse(fs.readFileSync(path.join(directory, name), 'utf8'));
      if (message.to === to && message.subject === subject) return message as { text: string };
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`No email "${subject}" was delivered to ${to}`);
}

/** The fixture as an MCP client and the platform owner meet it, the pairing window open. */
async function oauth(f: App, { paired = true } = {}) {
  // The server listens on loopback and names itself by its origin, as behind a tunnel.
  const server = `http://127.0.0.1:${f.env.PORT}`;
  const issuer = f.env.DISPATCH_ORIGIN!;
  const resource = `${issuer}/api/v1/mcp`;
  const challenge = `Bearer realm="Dispatch", resource_metadata="${issuer}/.well-known/oauth-protected-resource/api/v1/mcp", scope="dispatch"`;
  const owner = await f.client();
  if (paired) await openPairing(owner);
  const { dsps } = (await owner.read('/api/platform/agents')) as AgentKeys;
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  // The owner's browser: its session, and a cookie named for each request it brought, which
  // only that browser may see or answer. An answer clears that request's cookie alone.
  const session = owner.headers.cookie!;
  const requests = new Map<string, string>();
  const keep = (headers: Headers) => {
    const pair = (headers.get('set-cookie') ?? '').split(';')[0]!;
    const [name = '', nonce = ''] = pair.split('=');
    if (!name.startsWith('dispatch_oauth_request_')) return;
    if (nonce) requests.set(name, pair);
    else requests.delete(name);
    owner.headers.cookie = [session, ...requests.values()].join('; ');
  };

  const answer = async (response: Response): Promise<Answer> => {
    const text = await response.text();
    const json = response.headers.get('content-type')?.startsWith('application/json');
    return {
      status: response.status,
      headers: response.headers,
      body: json ? JSON.parse(text) : text,
    };
  };
  const get = async (path: string, headers: Record<string, string> = {}) =>
    answer(await fetch(server + path, { redirect: 'manual', headers }));
  /** A form post, as clients send to the token and revocation endpoints. */
  const form = async (path: string, fields: Record<string, string>) =>
    answer(
      await fetch(server + path, {
        method: 'POST',
        redirect: 'manual',
        headers: { 'content-type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams(fields).toString(),
      }),
    );
  /** Where the authorization endpoint sends the browser. */
  const authorize = async (query: Record<string, string> | [string, string][]) => {
    const response = await fetch(`${server}/oauth/authorize?${new URLSearchParams(query)}`, {
      redirect: 'manual',
    });
    const text = await response.text();
    assert.equal(response.status, 302, text);
    keep(response.headers);
    return response.headers.get('location')!;
  };
  /** An authorization request as a client sends one, with a PKCE pair and state of its own. */
  const request = (clientId: string, redirectUri: string, extra: Record<string, string> = {}) => {
    const { verifier, challenge } = pkce();
    const state = crypto.randomBytes(16).toString('base64url');
    const query: Record<string, string> = {
      response_type: 'code',
      client_id: clientId,
      redirect_uri: redirectUri,
      code_challenge: challenge,
      code_challenge_method: 'S256',
      state,
      resource,
      ...extra,
    };
    return { verifier, state, query };
  };
  const approvalPage = `${issuer}/#authorize?request=`;
  /** The request waiting on the approval page, from the browser's trip there. */
  const begin = async (
    clientId: string,
    redirectUri: string,
    extra: Record<string, string> = {},
  ) => {
    const sent = request(clientId, redirectUri, extra);
    const location = await authorize(sent.query);
    assert.ok(location.startsWith(approvalPage), location);
    return { ...sent, id: location.slice(approvalPage.length) };
  };
  /** The owner approves: where the browser goes back to the app. */
  const approve = async (id: string, choices: OAuthApproval) => {
    const approved = await owner.post(`/api/platform/oauth/requests/${id}/approve`, choices);
    assert.equal(approved.status, 200, approved.body);
    keep(approved.headers);
    return (approved.value as OAuthRedirect).redirect;
  };
  /** The code an approval's redirect carries, checked to bring back the state and issuer. */
  const code = (redirect: string, state: string) => {
    const back = params(redirect);
    assert.equal(back.get('state'), state, redirect);
    assert.equal(back.get('iss'), issuer, redirect);
    assert.ok(back.get('code'), redirect);
    return back.get('code')!;
  };
  const exchange = (clientId: string, redirectUri: string, grant: string, verifier: string) =>
    form('/oauth/token', {
      grant_type: 'authorization_code',
      code: grant,
      redirect_uri: redirectUri,
      code_verifier: verifier,
      client_id: clientId,
      resource,
    });
  const refresh = (clientId: string, token: string, extra: Record<string, string> = {}) =>
    form('/oauth/token', {
      grant_type: 'refresh_token',
      refresh_token: token,
      client_id: clientId,
      resource,
      ...extra,
    });
  /** An app from its authorization request to its first tokens. */
  const connect = async (clientId: string, redirectUri: string, choices: OAuthApproval) => {
    const started = await begin(clientId, redirectUri);
    const redirect = await approve(started.id, choices);
    const tokens = await exchange(
      clientId,
      redirectUri,
      code(redirect, started.state),
      started.verifier,
    );
    assert.equal(tokens.status, 200, JSON.stringify(tokens.body));
    return tokens.body as Tokens;
  };
  let id = 0;
  /** A JSON-RPC message to the MCP endpoint, as agent-mcp.test.ts sends one. */
  const rpc = async (token: string, method: string, rpcParams: object = {}) =>
    answer(
      await fetch(`${server}/api/v1/mcp`, {
        method: 'POST',
        redirect: 'manual',
        headers: {
          'content-type': 'application/json',
          accept: 'application/json, text/event-stream',
          'mcp-protocol-version': '2025-06-18',
          'user-agent': 'claude-code/2.1.283',
          authorization: `Bearer ${token}`,
        },
        body: JSON.stringify(
          method.startsWith('notifications/')
            ? { jsonrpc: '2.0', method, params: rpcParams }
            : { jsonrpc: '2.0', id: ++id, method, params: rpcParams },
        ),
      }),
    );
  const whoami = (token: string) => get('/api/v1/whoami', { authorization: `Bearer ${token}` });
  /** A token that no longer works: 401, and the challenge tells the client to sign in again. */
  const refused = (answered: Answer, what: string) => {
    assert.equal(answered.status, 401, `${what}: ${JSON.stringify(answered.body)}`);
    assert.equal(answered.headers.get('www-authenticate'), `${challenge}, error="invalid_token"`);
  };
  /** The connection of this name, as the Agents page lists it. */
  const listed = async (name: string) => {
    const { keys } = (await owner.read('/api/platform/agents')) as AgentKeys;
    return keys.filter((key) => key.name === name);
  };
  return {
    server,
    issuer,
    resource,
    challenge,
    owner,
    north,
    get,
    form,
    authorize,
    request,
    begin,
    approve,
    code,
    exchange,
    refresh,
    connect,
    rpc,
    whoami,
    refused,
    listed,
  };
}

test('an MCP client finds where to sign in, and Dispatch says exactly what it supports', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const { issuer, resource, challenge, get } = await oauth(f);
  assert.ok(!issuer.endsWith('/'));

  // Without a token the MCP endpoint stays a 401, never metadata, and names its metadata and
  // scope with no error, so the client starts signing in.
  const bare = await get('/api/v1/mcp');
  assert.equal(bare.status, 401);
  const header = bare.headers.get('www-authenticate') ?? '';
  assert.equal(header, challenge);
  assert.match(header, / resource_metadata="[^"]+"/);
  assert.match(header, / scope="dispatch"/);

  const protectedResource = {
    resource,
    authorization_servers: [issuer],
    scopes_supported: ['dispatch'],
    bearer_methods_supported: ['header'],
    resource_name: 'Dispatch',
  };
  const authorizationServer = {
    issuer,
    authorization_endpoint: `${issuer}/oauth/authorize`,
    token_endpoint: `${issuer}/oauth/token`,
    registration_endpoint: `${issuer}/oauth/register`,
    revocation_endpoint: `${issuer}/oauth/revoke`,
    response_types_supported: ['code'],
    response_modes_supported: ['query'],
    grant_types_supported: ['authorization_code', 'refresh_token'],
    code_challenge_methods_supported: ['S256'],
    token_endpoint_auth_methods_supported: ['none'],
    revocation_endpoint_auth_methods_supported: ['none'],
    scopes_supported: ['dispatch'],
    client_id_metadata_document_supported: true,
    authorization_response_iss_parameter_supported: true,
  };
  for (const [path, expected] of [
    ['/.well-known/oauth-protected-resource', protectedResource],
    ['/.well-known/oauth-protected-resource/api/v1/mcp', protectedResource],
    ['/.well-known/oauth-authorization-server', authorizationServer],
    ['/.well-known/oauth-authorization-server/api/v1/mcp', authorizationServer],
  ] as const) {
    const document = await get(path);
    assert.equal(document.status, 200, path);
    assert.match(document.headers.get('content-type') ?? '', /^application\/json/, path);
    assert.deepEqual(document.body, expected, path);
  }
  // The challenge points at a document that is really there, on the issuer's origin.
  const named = / resource_metadata="([^"]+)"/.exec(header)![1]!;
  assert.ok(named.startsWith(`${issuer}/`), named);
  assert.deepEqual((await get(named.slice(issuer.length))).body, protectedResource);

  // No OpenID document: Claude Code would reject an incomplete one.
  assert.equal((await get('/.well-known/openid-configuration')).status, 404);
});

test('Claude Code connects by its published document and reaches only what the owner chose', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  // Claude Code listens on a port of its own choosing; its document lists the portless URL.
  const callback = `http://localhost:${await freePort()}/callback`;
  const started = await c.begin(claudeCode, callback, {
    scope: 'dispatch',
    ui_locales: 'en-US',
    ba_param: 'anything',
  });

  // The owner reads what is asking, then approves it with a narrower reach.
  const shown = await c.owner.get(`/api/platform/oauth/requests/${started.id}`);
  assert.equal(shown.status, 200, shown.body);
  const waiting = shown.value as OAuthRequest;
  assert.deepEqual(waiting.app, {
    name: 'Claude Code',
    clientId: claudeCode,
    known: true,
    redirectHost: 'this computer',
    redirectScheme: null,
  });
  assert.equal(waiting.id, started.id);
  const expires = Date.parse(waiting.expiresAt) - Date.now();
  assert.ok(expires > 9 * 60_000 && expires <= 10 * 60_000, waiting.expiresAt);
  const redirect = await c.approve(started.id, {
    name: 'Laptop – Claude Code',
    allDsps: false,
    dsps: [c.north.id],
    reads: routesAndDvic,
  });
  assert.ok(redirect.startsWith(`${callback}?`), redirect);
  const code = c.code(redirect, started.state);

  const tokens = await c.exchange(claudeCode, callback, code, started.verifier);
  assert.equal(tokens.status, 200, JSON.stringify(tokens.body));
  assert.equal(tokens.headers.get('cache-control'), 'no-store');
  assert.equal(tokens.headers.get('pragma'), 'no-cache');
  const issued = tokens.body as Tokens;
  assert.deepEqual(Object.keys(issued).sort(), [
    'access_token',
    'expires_in',
    'refresh_token',
    'scope',
    'token_type',
  ]);
  assert.equal(issued.token_type, 'Bearer');
  assert.equal(issued.expires_in, 3600);
  assert.equal(issued.scope, 'dispatch');
  assert.match(issued.access_token, /^dsa_dev_[0-9A-Za-z]{38}$/);
  assert.match(issued.refresh_token, /^dsr_dev_[0-9A-Za-z]{38}$/);

  // The access token speaks MCP like a key with the same choices.
  const access = issued.access_token;
  const hello = await c.rpc(access, 'initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: 'claude-code', version: '2.1.283' },
  });
  assert.equal(hello.status, 200, JSON.stringify(hello.body));
  assert.equal(hello.body.result.protocolVersion, '2025-06-18');
  assert.equal(hello.body.result.serverInfo.name, 'dispatch');
  assert.equal((await c.rpc(access, 'notifications/initialized')).status, 202);
  const listed = await c.rpc(access, 'tools/list');
  assert.equal(listed.status, 200, JSON.stringify(listed.body));
  const tools = (listed.body.result.tools as { name: string }[]).map((tool) => tool.name);
  assert.deepEqual(tools.sort(), routesAndDvicTools);
  const called = await c.rpc(access, 'tools/call', { name: 'whoami', arguments: {} });
  assert.equal(called.body.result.isError, false, JSON.stringify(called.body));
  const me = called.body.result.structuredContent as AgentWhoami;
  assert.deepEqual(JSON.parse(called.body.result.content[0].text), me);
  assert.deepEqual(me.key, { name: 'Laptop – Claude Code', access: 'read', expiresAt: null });
  assert.deepEqual(
    me.dsps.map((dsp) => [dsp.id, dsp.reads]),
    [[c.north.id, routesAndDvic]],
  );

  // The same code presented again, with everything right, ends the app it made.
  const replay = await c.exchange(claudeCode, callback, code, started.verifier);
  assert.deepEqual([replay.status, replay.body.error], [400, 'invalid_grant']);
  c.refused(await c.rpc(access, 'tools/list'), 'MCP after the replay');
  c.refused(await c.whoami(access), 'whoami after the replay');
  const [app] = await c.listed('Laptop – Claude Code');
  assert.ok(app?.revokedAt, 'the replayed code revokes the app');
});

test('refresh tokens rotate once, and replay ends the entire connection immediately', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const callback = `http://127.0.0.1:${await freePort()}/callback`;
  const first = await c.connect(claudeCode, callback, everything('Desktop – Claude Code'));

  // Rotated: a new pair in the same shape, with any extra scope asked for ignored.
  const second = await c.refresh(claudeCode, first.refresh_token, {
    scope: 'dispatch offline_access',
  });
  assert.equal(second.status, 200, JSON.stringify(second.body));
  assert.equal(second.headers.get('cache-control'), 'no-store');
  const rotated = second.body as Tokens;
  assert.equal(rotated.token_type, 'Bearer');
  assert.equal(rotated.expires_in, 3600);
  assert.equal(rotated.scope, 'dispatch');
  assert.notEqual(rotated.access_token, first.access_token);
  assert.notEqual(rotated.refresh_token, first.refresh_token);
  // The old access token is not ended by the rotation; it lasts out its hour.
  assert.equal((await c.whoami(first.access_token)).status, 200);
  assert.equal((await c.whoami(rotated.access_token)).status, 200);

  // Reusing the consumed credential is treated as theft, not as a parallel refresh grace.
  const replay = await c.refresh(claudeCode, first.refresh_token);
  assert.deepEqual([replay.status, replay.body.error], [400, 'invalid_grant']);
  c.refused(await c.whoami(first.access_token), 'access before the replay');
  c.refused(await c.whoami(rotated.access_token), 'access minted by the replayed family');
  const successor = await c.refresh(claudeCode, rotated.refresh_token);
  assert.deepEqual([successor.status, successor.body.error], [400, 'invalid_grant']);
  const apps = await c.listed('Desktop – Claude Code');
  assert.equal(apps.length, 1);
  assert.ok(apps[0]!.revokedAt);
});

test('an app signing out, or the owner revoking it, ends its access at once', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const callback = `http://localhost:${await freePort()}/callback`;

  // Claude Code's `mcp logout` revokes its refresh token.
  const laptop = await c.connect(claudeCode, callback, everything('Laptop – Claude Code'));
  const [listed] = await c.listed('Laptop – Claude Code');
  assert.ok(listed);
  const app: AgentKey = listed;
  assert.equal(app.kind, 'app');
  assert.deepEqual(app.client, { name: 'Claude Code', known: true, status: 'connected' });
  assert.equal(app.hint, '');
  assert.equal(app.access, 'read');
  assert.equal(app.revokedAt, null);
  const signedOut = await c.form('/oauth/revoke', {
    token: laptop.refresh_token,
    token_type_hint: 'refresh_token',
    client_id: claudeCode,
  });
  assert.equal(signedOut.status, 200, JSON.stringify(signedOut.body));
  c.refused(await c.rpc(laptop.access_token, 'tools/list'), 'MCP after signing out');
  const renewed = await c.refresh(claudeCode, laptop.refresh_token);
  assert.deepEqual([renewed.status, renewed.body.error], [400, 'invalid_grant']);
  assert.ok((await c.listed('Laptop – Claude Code'))[0]!.revokedAt);
  // A token Dispatch never issued is no error.
  const unknown = await c.form('/oauth/revoke', { token: 'dsr_dev_unknown' });
  assert.equal(unknown.status, 200);

  // The owner revokes another connection from the Agents page.
  const desktop = await c.connect(claudeCode, callback, everything('Desktop – Claude Code'));
  assert.equal((await c.whoami(desktop.access_token)).status, 200);
  const [connected] = await c.listed('Desktop – Claude Code');
  assert.equal(connected?.kind, 'app');
  const revoked = await c.owner.post(`/api/platform/agents/keys/${connected!.id}/revoke`);
  assert.equal(revoked.status, 200, revoked.body);
  c.refused(await c.rpc(desktop.access_token, 'tools/list'), 'MCP after the owner revoked it');
  c.refused(await c.whoami(desktop.access_token), 'whoami after the owner revoked it');
  const after = await c.refresh(claudeCode, desktop.refresh_token);
  assert.deepEqual([after.status, after.body.error], [400, 'invalid_grant']);
});

test('connecting the same app again under the same name replaces the earlier connection', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const callback = `http://localhost:${await freePort()}/callback`;
  const earlier = await c.connect(claudeCode, callback, everything('Laptop – Claude Code'));
  const other = await c.connect(claudeCode, callback, everything('Desktop – Claude Code'));
  const [first] = await c.listed('Laptop – Claude Code');

  // Claude Code signs in again, as after its tokens were lost; the owner approves it under
  // the same name, now reading routes and DVIC alone.
  const again = await c.connect(claudeCode, callback, {
    ...everything('Laptop – Claude Code'),
    reads: routesAndDvic,
  });
  c.refused(await c.whoami(earlier.access_token), 'the replaced connection');
  const renewed = await c.refresh(claudeCode, earlier.refresh_token);
  assert.deepEqual([renewed.status, renewed.body.error], [400, 'invalid_grant']);
  const me = await c.whoami(again.access_token);
  assert.equal(me.status, 200, JSON.stringify(me.body));
  assert.deepEqual((me.body as AgentWhoami).dsps[0]!.reads, routesAndDvic);

  // One live connection of that name, the new one; the earlier one is listed as revoked.
  const laptops = await c.listed('Laptop – Claude Code');
  const live = laptops.filter((key) => key.revokedAt === null);
  assert.equal(live.length, 1);
  assert.notEqual(live[0]!.id, first!.id);
  assert.deepEqual(live[0]!.reads, routesAndDvic);
  assert.ok(laptops.find((key) => key.id === first!.id)?.revokedAt);
  // A connection of the same app under another name is left alone.
  assert.equal((await c.whoami(other.access_token)).status, 200);
});

test('a bad authorization request never reaches an unknown app, and a known one hears why', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const callback = `http://localhost:${await freePort()}/callback`;
  const page = (error: string) => `${c.issuer}/#authorize?error=${error}`;

  // An app Dispatch doesn't know is never sent anything, even to the redirect it names.
  const stranger = c.request('https://evil.example/client.json', 'https://evil.example/cb');
  assert.equal(await c.authorize(stranger.query), page('unknown_app'));
  // A known app sent somewhere its document doesn't list is not sent there.
  const elsewhere = c.request(claudeCode, 'https://evil.example/callback');
  assert.equal(await c.authorize(elsewhere.query), page('invalid_redirect'));

  const missingResource = c.request(claudeCode, callback);
  delete missingResource.query.resource;
  const withoutResource = params(await c.authorize(missingResource.query));
  assert.equal(withoutResource.get('error'), 'invalid_target');

  // Once the app and its redirect are good, the app hears what was wrong, with its state
  // and the issuer to check.
  for (const [change, error] of [
    [{ code_challenge: '' }, 'invalid_request'],
    [{ code_challenge_method: 'plain' }, 'invalid_request'],
    [{ scope: 'offline_access' }, 'invalid_scope'],
    [{ scope: 'dispatch admin' }, 'invalid_scope'],
    [{ resource: `${c.issuer}/api/v1` }, 'invalid_target'],
    [{ resource: 'https://evil.example/api/v1/mcp' }, 'invalid_target'],
  ] as const) {
    const sent = c.request(claudeCode, callback);
    const query = Object.entries({ ...sent.query, ...change }).filter(([, value]) => value);
    const location = await c.authorize(query);
    assert.ok(location.startsWith(`${callback}?`), location);
    const back = params(location);
    assert.equal(back.get('error'), error, location);
    assert.ok(back.get('error_description'), location);
    assert.equal(back.get('state'), sent.state, location);
    assert.equal(back.get('iss'), c.issuer, location);
    assert.equal(back.get('code'), null, location);
  }
});

test('apps register themselves only to come back to this computer', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const register = async (metadata: object) => {
    const response = await fetch(`${c.server}/oauth/register`, {
      method: 'POST',
      redirect: 'manual',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(metadata),
    });
    return { status: response.status, headers: response.headers, body: await response.json() };
  };
  const loopback = `http://127.0.0.1:${await freePort()}/callback`;
  const local = await register({
    client_name: 'mcp-remote',
    redirect_uris: [loopback],
    token_endpoint_auth_method: 'none',
    grant_types: ['authorization_code', 'refresh_token'],
    response_types: ['code'],
  });
  assert.equal(local.status, 201, JSON.stringify(local.body));
  assert.equal(local.headers.get('cache-control'), 'no-store');
  assert.match(local.body.client_id, /^dcr_/);
  assert.deepEqual(local.body.redirect_uris, [loopback]);
  assert.equal(local.body.token_endpoint_auth_method, 'none');
  const cursor = await register({ client_name: 'Cursor', redirect_uris: [cursorRedirect] });
  assert.equal(cursor.status, 201, JSON.stringify(cursor.body));
  for (const uri of ['https://evil.example/cb', 'javascript:alert(1)']) {
    const refused = await register({ client_name: 'Evil', redirect_uris: [uri] });
    assert.deepEqual([refused.status, refused.body.error], [400, 'invalid_redirect_uri'], uri);
  }

  // Cursor goes all the way: app-provided metadata, sent back to itself by its own scheme.
  const clientId = cursor.body.client_id as string;
  const started = await c.begin(clientId, cursorRedirect);
  const shown = await c.owner.get(`/api/platform/oauth/requests/${started.id}`);
  assert.equal(shown.status, 200, shown.body);
  assert.deepEqual((shown.value as OAuthRequest).app, {
    name: 'Cursor',
    clientId,
    known: false,
    redirectHost: 'this computer',
    redirectScheme: 'cursor',
  });
  const redirect = await c.approve(started.id, everything('Cursor'));
  assert.ok(redirect.startsWith(`${cursorRedirect}?`), redirect);
  const tokens = await c.exchange(
    clientId,
    cursorRedirect,
    c.code(redirect, started.state),
    started.verifier,
  );
  assert.equal(tokens.status, 200, JSON.stringify(tokens.body));
  const me = await c.whoami((tokens.body as Tokens).access_token);
  assert.equal(me.status, 200, JSON.stringify(me.body));
  assert.equal((me.body as AgentWhoami).key.name, 'Cursor');
  const [app] = await c.listed('Cursor');
  assert.equal(app?.kind, 'app');
  assert.deepEqual(app?.client, { name: 'Cursor', known: false, status: 'connected' });
});

test('ChatGPT connects through its own redirect and no other', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const started = await c.begin(chatgpt, chatgptRedirect, { ui_locales: 'en-US' });
  const shown = await c.owner.get(`/api/platform/oauth/requests/${started.id}`);
  assert.deepEqual((shown.value as OAuthRequest).app, {
    name: 'ChatGPT',
    clientId: chatgpt,
    known: true,
    redirectHost: 'chatgpt.com',
    redirectScheme: null,
  });
  const redirect = await c.approve(started.id, everything('ChatGPT'));
  assert.ok(redirect.startsWith(`${chatgptRedirect}?`), redirect);
  const tokens = await c.exchange(
    chatgpt,
    chatgptRedirect,
    c.code(redirect, started.state),
    started.verifier,
  );
  assert.equal(tokens.status, 200, JSON.stringify(tokens.body));
  assert.equal((await c.whoami((tokens.body as Tokens).access_token)).status, 200);

  // Any other chatgpt.com address is not ChatGPT's redirect.
  for (const other of [
    'https://chatgpt.com/other_oauth_redirect',
    `${chatgptRedirect}/`,
    'https://chatgpt.com:443/connector_platform_oauth_redirect',
  ]) {
    const sent = c.request(chatgpt, other);
    assert.equal(
      await c.authorize(sent.query),
      `${c.issuer}/#authorize?error=invalid_redirect`,
      other,
    );
  }
});

test('apps ask to connect only while the owner has the pairing window open', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f, { paired: false });
  const callback = `http://localhost:${await freePort()}/callback`;
  const page = (error: string) => `${c.issuer}/#authorize?error=${error}`;

  // Closed: the browser lands on Dispatch's page, never at the app.
  assert.deepEqual((await c.owner.read('/api/platform/oauth/pairing')) as OAuthPairing, {
    openUntil: null,
  });
  assert.equal(await c.authorize(c.request(claudeCode, callback).query), page('pairing_closed'));

  // The owner opens it for ten minutes, and the app asks again.
  const openUntil = await openPairing(c.owner);
  const left = Date.parse(openUntil) - Date.now();
  assert.ok(left > 9 * 60_000 && left <= 10 * 60_000, openUntil);
  assert.deepEqual((await c.owner.read('/api/platform/oauth/pairing')) as OAuthPairing, {
    openUntil,
  });
  const tokens = await c.connect(claudeCode, callback, everything('Laptop – Claude Code'));
  assert.equal((await c.whoami(tokens.access_token)).status, 200);

  // A kind of app the owner turns off is refused by name, while others still ask.
  const apps = (await c.owner.read('/api/platform/oauth/apps')) as OAuthAllowedApps;
  assert.deepEqual(
    apps.apps.map((app) => [app.id, app.allowed]),
    [
      ['chatgpt', true],
      ['codex', true],
      ['claude-code', true],
      ['hermes', true],
      ['local', true],
      ['web', false],
    ],
  );
  const off = await c.owner.post('/api/platform/oauth/apps', { id: 'claude-code', allowed: false });
  assert.equal(off.status, 200, off.body);
  assert.equal(
    await c.authorize(c.request(claudeCode, callback).query),
    `${page('app_not_allowed')}&app=claude-code`,
  );
  assert.ok((await c.begin(chatgpt, chatgptRedirect)).id);
  // Websites stay unknown until the owner lets them connect.
  assert.equal(
    await c.authorize(c.request('https://app.dispatch.test/oauth/client.json', callback).query),
    page('unknown_app'),
  );
  // Renewing never needs the window or the app's kind turned on.
  const renewed = await c.refresh(claudeCode, tokens.refresh_token);
  assert.equal(renewed.status, 200, JSON.stringify(renewed.body));
});

test('the platform owner is emailed when an app connects and when Dispatch disconnects it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const callback = `http://127.0.0.1:${await freePort()}/callback`;
  const owner = c.owner.session.user.email as string;

  const tokens = await c.connect(claudeCode, callback, {
    name: 'Laptop – Claude Code',
    allDsps: false,
    dsps: [c.north.id],
    reads: routesAndDvic,
  });
  const connected = await capturedMail(f.root, owner);
  assert.equal(connected.subject, '[Dispatch Dev] Claude Code connected to Dispatch');
  for (const line of [
    'Connection: Laptop – Claude Code',
    'App: Claude Code (known metadata)',
    'Sends access to: this computer',
    'DSPs: Northline Logistics',
    'Access: Reads Routes & packages, DVIC inspections',
    `${c.issuer}/#agents?tab=apps`,
  ]) {
    assert.ok(connected.text.includes(line), `${line}\n${connected.text}`);
  }
  assert.ok(connected.html.includes('Review connected apps'));

  // The connection's own code presented again: Dispatch ends it and says why.
  const started = await c.begin(chatgpt, chatgptRedirect);
  const code = c.code(await c.approve(started.id, everything('ChatGPT')), started.state);
  const exchanged = await c.exchange(chatgpt, chatgptRedirect, code, started.verifier);
  assert.equal(exchanged.status, 200, JSON.stringify(exchanged.body));
  await mailed(f.root, owner, '[Dispatch Dev] ChatGPT connected to Dispatch');
  const replay = await c.exchange(chatgpt, chatgptRedirect, code, started.verifier);
  assert.equal(replay.body.error, 'invalid_grant');
  const disconnected = await mailed(f.root, owner, '[Dispatch Dev] Dispatch disconnected ChatGPT');
  assert.ok(disconnected.text.includes('one-time code'), disconnected.text);
  assert.ok(disconnected.text.includes('Sent access to: chatgpt.com'), disconnected.text);
  c.refused(await c.whoami((exchanged.body as Tokens).access_token), 'the replayed app');
  assert.equal((await c.whoami(tokens.access_token)).status, 200);
});

test('a request opens only in the browser the app sent to Dispatch', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const c = await oauth(f);
  const callback = `http://localhost:${await freePort()}/callback`;
  const started = await c.begin(claudeCode, callback);
  const path = `/api/platform/oauth/requests/${started.id}`;
  // The browser the app sent holds the request; the same owner signed in anywhere else, as
  // from a link someone sent them, does not.
  const held = c.owner.headers.cookie!;
  assert.match(held, new RegExp(`; dispatch_oauth_request_${started.id}=[\\w-]{43}$`));
  const elsewhere = await f.client();
  for (const answered of [
    await elsewhere.get(path),
    await elsewhere.post(`${path}/approve`, everything('Claude Code')),
    await elsewhere.post(`${path}/deny`),
  ]) {
    assert.deepEqual([answered.status, answered.value.error], [403, 'wrong_browser']);
  }
  assert.equal((await c.owner.get(path)).status, 200);
  // A second app started in the same browser before the first is approved: both open, and
  // each approval clears only its own cookie.
  const second = await c.begin(chatgpt, chatgptRedirect);
  assert.equal((await c.owner.get(`/api/platform/oauth/requests/${second.id}`)).status, 200);
  assert.equal((await c.owner.get(path)).status, 200);
  const redirect = await c.approve(started.id, everything('Claude Code'));
  assert.ok(redirect.startsWith(`${callback}?`), redirect);
  assert.ok(!c.owner.headers.cookie!.includes(started.id));
  assert.ok(c.owner.headers.cookie!.includes(second.id));
  const after = await c.owner.get(path);
  assert.deepEqual([after.status, after.value.error], [403, 'wrong_browser']);
  await c.approve(second.id, everything('ChatGPT'));
  assert.equal(c.owner.headers.cookie, held.split('; dispatch_oauth_request_')[0]);
});
