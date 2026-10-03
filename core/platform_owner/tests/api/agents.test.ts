import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../shell/tests/support/support.js';
import type {
  AgentKeyCreated,
  AgentKeyRequest,
  AgentKeys,
  AgentWhoami,
} from '../../../../shared/contracts/platform-owner.js';

const body = (dsps: string[]): AgentKeyRequest => ({
  name: 'Laptop – Claude Code',
  allDsps: dsps.length === 0,
  dsps,
  access: 'read',
  reads: {
    areas: [
      'routes',
      'timecards',
      'meal_breaks',
      'dvic',
      'feedback',
      'safety',
      'returns',
      'scorecard',
    ],
    bypass: false,
  },
  dspReads: [],
  expiresAt: null,
});

test('only the platform owner makes keys, and a key reaches only what it was given', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get('/api/platform/agents')).status, 403);
  assert.equal((await member.post('/api/platform/agents/keys', body([]))).status, 403);
  const listed: AgentKeys = (await owner.get('/api/platform/agents')).value;
  const north = listed.dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const made = await owner.post('/api/platform/agents/keys', body([north.id]));
  assert.equal(made.status, 200, made.body);
  const { key, token }: AgentKeyCreated = made.value;
  assert.match(token, /^dsk_dev_[0-9A-Za-z]{38}$/);
  assert.equal(key.hint, token.slice(-4));
  // The list never carries a key again.
  const again = await owner.get('/api/platform/agents');
  assert.ok(!again.body.includes(token));

  // The key alone signs an agent in, as a bearer token or an API key header.
  const bearer = { authorization: `Bearer ${token}` };
  const whoami = await f.request('/api/v1/whoami', undefined, bearer);
  assert.equal(whoami.status, 200, whoami.body);
  const me: AgentWhoami = whoami.value;
  assert.equal(me.key.access, 'read');
  assert.deepEqual(
    me.dsps.map((dsp) => dsp.name),
    ['Northline Logistics'],
  );
  assert.match(me.dsps[0]!.today, /^\d{4}-\d{2}-\d{2}$/);
  assert.equal((await f.request('/api/v1/whoami', undefined, { 'x-api-key': token })).status, 200);

  // Without a key the answer says how to sign in; a session never reaches the agent API.
  const bare = await f.raw('/api/v1/whoami');
  assert.equal(bare.status, 401);
  assert.match(bare.headers.get('www-authenticate') ?? '', /^Bearer realm="Dispatch"/);
  assert.equal((await owner.get('/api/v1/whoami')).status, 401);
  const cookie = await f.request('/api/v1/whoami', undefined, {
    ...bearer,
    cookie: owner.headers.cookie!,
  });
  assert.deepEqual([cookie.status, cookie.value.error], [400, 'session_and_key']);
  // And a key reaches none of the dashboard's routes.
  assert.equal((await f.request('/api/platform/agents', undefined, bearer)).status, 401);
  assert.equal((await f.request('/api/v1/nothing-here', undefined, bearer)).status, 404);
  assert.equal((await f.raw('/api/v1/nothing-here')).status, 401);

  // A changed key changes what the agent sees on its next call.
  const wider = await owner.post(`/api/platform/agents/keys/${key.id}`, body([]));
  assert.equal(wider.status, 200, wider.body);
  const all: AgentWhoami = (await f.request('/api/v1/whoami', undefined, bearer)).value;
  assert.ok(all.dsps.length >= 1 && all.dsps.some((dsp) => dsp.name === 'Northline Logistics'));
  const taken = await owner.post('/api/platform/agents/keys', body([]));
  assert.deepEqual([taken.status, taken.value.error], [409, 'agent_key_name_taken']);

  // Revoked, the key stops at once; revoking everything ends the rest.
  assert.equal((await owner.post(`/api/platform/agents/keys/${key.id}/revoke`)).status, 200);
  const revoked = await f.request('/api/v1/whoami', undefined, bearer);
  assert.deepEqual([revoked.status, revoked.value.error], [401, 'agent_key_revoked']);
  const second: AgentKeyCreated = (
    await owner.post('/api/platform/agents/keys', { ...body([]), name: 'Desktop – Codex' })
  ).value;
  const ended = await owner.post('/api/platform/agents/revoke-all');
  assert.equal(ended.value.revoked, 1);
  assert.equal(
    (await f.request('/api/v1/whoami', undefined, { authorization: `Bearer ${second.token}` }))
      .status,
    401,
  );

  // The platform's log records each step; no DSP's does.
  const log = (await owner.get('/api/platform/audit?limit=50')).value.events as {
    action: string;
    dspId: string | null;
  }[];
  const agentEvents = log.filter((event) => event.action.startsWith('agent.'));
  assert.deepEqual(
    agentEvents.map((event) => event.action),
    [
      'agent.keys_revoked',
      'agent.key_created',
      'agent.key_revoked',
      'agent.key_updated',
      'agent.key_created',
    ],
  );
  assert.ok(agentEvents.every((event) => event.dspId === null));
});
