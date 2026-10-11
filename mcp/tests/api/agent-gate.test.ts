import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { fixture } from '../../../core/shell/tests/support/support.js';
import type { AgentKeyCreated } from '../../api/index.js';

// Each agent's requests come from an address of its own, as they do on the internet.
function from(
  address: string,
  origin: string,
  path: string,
  headers: Record<string, string>,
  body?: string,
) {
  return new Promise<{ status: number; retryAfter: string | undefined; value: any }>(
    (resolve, reject) => {
      const request = http.request(
        origin + path,
        { method: body === undefined ? 'GET' : 'POST', headers, localAddress: address },
        (response) => {
          let text = '';
          response.setEncoding('utf8');
          response.on('data', (chunk) => (text += chunk));
          response.on('end', () =>
            resolve({
              status: response.statusCode!,
              retryAfter: response.headers['retry-after'],
              value: JSON.parse(text),
            }),
          );
        },
      );
      request.once('error', reject);
      request.end(body);
    },
  );
}

test('made-up keys never reach the database, and an address presenting them waits out the minute', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const origin = `http://127.0.0.1:${f.env.PORT}`;
  const owner = await f.client();
  const key = (): any => ({
    name: `Laptop ${Math.random()}`,
    allDsps: true,
    dsps: [],
    access: 'read',
    allTools: true,
    tools: {},
    expiresAt: null,
  });
  const good: AgentKeyCreated = (await owner.post('/api/platform/agents/keys', key())).value;
  const gone: AgentKeyCreated = (await owner.post('/api/platform/agents/keys', key())).value;
  assert.equal((await owner.post(`/api/platform/agents/keys/${gone.key.id}/revoke`)).status, 200);
  const bearer = (token: string) => ({ authorization: `Bearer ${token}` });

  // A key not shaped like one is refused at once, before the database; one shaped like a
  // key but revoked is refused by the database. Each counts against the address.
  const malformed = await from(
    '127.0.0.2',
    origin,
    '/api/v1/whoami',
    bearer('dsk_dev_' + 'A'.repeat(38)),
  );
  assert.deepEqual([malformed.status, malformed.value.error], [401, 'agent_key_invalid']);
  for (let i = 1; i < 20; i++) {
    const refused = await from('127.0.0.2', origin, '/api/v1/whoami', bearer(gone.token));
    assert.deepEqual(
      [refused.status, refused.value.error],
      [401, 'agent_key_revoked'],
      `call ${i}`,
    );
  }
  // Twenty refused in a minute: the address waits, with the key that works included, on
  // every path of the agent API; another address is unaffected.
  const waiting = await from('127.0.0.2', origin, '/api/v1/whoami', bearer(gone.token));
  assert.deepEqual(
    [waiting.status, waiting.value.error, waiting.retryAfter],
    [429, 'rate_limited', '60'],
  );
  assert.equal((await from('127.0.0.2', origin, '/api/v1/whoami', bearer(good.token))).status, 429);
  const mcp = await from(
    '127.0.0.2',
    origin,
    '/api/v1/mcp',
    {
      ...bearer(good.token),
      'content-type': 'application/json',
      accept: 'application/json, text/event-stream',
    },
    JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' }),
  );
  assert.equal(mcp.status, 429);
  assert.equal(
    (await from('127.0.0.2', origin, '/api/v1/nothing-here', bearer(good.token))).status,
    429,
  );
  assert.equal((await from('127.0.0.3', origin, '/api/v1/whoami', bearer(good.token))).status, 200);
});
