import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, until } from '../../../shell/tests/support/support.js';
import type {
  AgentActivityPage,
  AgentKeyCreated,
  AgentKeys,
} from '../../../platform_owner/api/index.js';

test('the Activity log lists what a key called over REST and MCP, written down in batches', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const { dsps }: AgentKeys = (await owner.get('/api/platform/agents')).value;
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const made = await owner.post('/api/platform/agents/keys', {
    name: 'Laptop – Claude Code',
    allDsps: false,
    dsps: [north.id],
    access: 'read',
    expiresAt: null,
  });
  assert.equal(made.status, 200, made.body);
  const { key, token }: AgentKeyCreated = made.value;
  const bearer = { authorization: `Bearer ${token}`, 'user-agent': 'claude-code/2.1.283' };

  const status = await f.raw('/api/v1/whoami', undefined, bearer);
  const answered = await status.text();
  assert.equal(status.status, 200, answered);
  const mcp = await fetch(`http://127.0.0.1:${f.env.PORT}/api/v1/mcp`, {
    method: 'POST',
    headers: {
      ...bearer,
      'content-type': 'application/json',
      accept: 'application/json, text/event-stream',
      'mcp-protocol-version': '2025-06-18',
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method: 'tools/call',
      params: { name: 'whoami', arguments: { driver: 'Nobody Anywhere' } },
    }),
  });
  const called = await mcp.json();
  assert.equal(called.result.isError, true, JSON.stringify(called));
  assert.match(called.result.content[0].text, /^unknown_parameter/);

  // The scheduler writes calls down every few seconds; until then the log has none of them.
  let page: AgentActivityPage = { rows: [], next: null };
  await until(async () => {
    page = await owner.read(`/api/platform/agents/activity?key=${key.id}`);
    return page.rows.length === 2;
  }, 20000);
  const [tool, rest] = page.rows;
  assert.deepEqual(
    page.rows.map((row) => [row.surface, row.outcome]),
    [
      ['mcp:whoami', 'unknown_parameter'],
      ['rest:whoami', 'ok'],
    ],
  );
  assert.deepEqual(rest!.key, { id: key.id, name: 'Laptop – Claude Code', kind: 'key' });
  assert.equal(rest!.bytes, Buffer.byteLength(answered));
  assert.ok(rest!.ms >= 0 && tool!.bytes > 0);
  assert.ok(Date.parse(tool!.at) >= Date.parse(rest!.at));
  assert.equal(page.next, null);
  // Never what the agent asked beyond the tool, nor its key.
  const text = JSON.stringify(page);
  assert.ok(!text.includes('Nobody') && !text.includes(token));

  // Narrowed to what was refused, and paged.
  const refused: AgentActivityPage = await owner.read(
    `/api/platform/agents/activity?key=${key.id}&outcome=refused`,
  );
  assert.deepEqual(
    refused.rows.map((row) => row.surface),
    ['mcp:whoami'],
  );
  const first: AgentActivityPage = await owner.read(
    `/api/platform/agents/activity?key=${key.id}&limit=1`,
  );
  assert.equal(first.rows[0]!.surface, 'mcp:whoami');
  const second: AgentActivityPage = await owner.read(
    `/api/platform/agents/activity?key=${key.id}&limit=1&before=${first.next}`,
  );
  assert.deepEqual([second.rows[0]!.surface, second.next], ['rest:whoami', null]);
  assert.equal((await owner.get('/api/platform/agents/activity?outcome=maybe')).status, 400);

  // Only a platform owner reads it.
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get('/api/platform/agents/activity')).status, 403);
  assert.equal((await f.request('/api/platform/agents/activity', undefined, bearer)).status, 401);
});
