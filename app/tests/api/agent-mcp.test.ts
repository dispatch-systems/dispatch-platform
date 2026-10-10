import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';
import type { AgentKeyCreated, AgentKeys } from '../../../mcp/api/index.js';

type Rpc = { status: number; headers: Headers; body: any };

test('any MCP client connects with a key, on every protocol version', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const { dsps }: AgentKeys = (await owner.get('/api/platform/agents')).value;
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const key = async (name: string) => {
    const made = await owner.post('/api/platform/agents/keys', {
      name,
      allDsps: false,
      dsps: [north.id],
      access: 'read',
      allTools: true,
      tools: {},
      expiresAt: null,
    });
    assert.equal(made.status, 200, made.body);
    return (made.value as AgentKeyCreated).token;
  };
  // The server listens on loopback and names itself by its origin, as behind a tunnel.
  const server = `http://127.0.0.1:${f.env.PORT}`;
  const full = await key('Laptop – Claude Code');
  let id = 0;
  const rpc = async (
    token: string | null,
    method: string,
    params: object = {},
    headers: Record<string, string> = {},
  ): Promise<Rpc> => {
    const response = await fetch(`${server}/api/v1/mcp`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        accept: 'application/json, text/event-stream',
        'user-agent': 'claude-code/2.1.283',
        ...(token ? { authorization: `Bearer ${token}` } : {}),
        ...headers,
      },
      body: JSON.stringify(
        method.startsWith('notifications/')
          ? { jsonrpc: '2.0', method, params }
          : { jsonrpc: '2.0', id: ++id, method, params },
      ),
    });
    const text = await response.text();
    return {
      status: response.status,
      headers: response.headers,
      // JSON-RPC and Dispatch answer JSON; a refused browser gets plain text.
      body: response.headers.get('content-type')?.startsWith('application/json')
        ? JSON.parse(text)
        : text,
    };
  };
  const call = async (token: string, name: string, args: object, version = '2025-06-18') =>
    (
      await rpc(
        token,
        'tools/call',
        { name, arguments: args },
        {
          'mcp-protocol-version': version,
        },
      )
    ).body.result;

  // The handshake every client before 2026-07-28 makes, with no session to keep.
  const hello = await rpc(full, 'initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: 'claude-code', version: '2.1.283' },
  });
  assert.equal(hello.status, 200, JSON.stringify(hello.body));
  assert.equal(hello.headers.get('mcp-session-id'), null);
  assert.equal(hello.body.result.protocolVersion, '2025-06-18');
  assert.equal(hello.body.result.serverInfo.name, 'dispatch');
  assert.ok(hello.body.result.capabilities.tools);
  assert.equal((await rpc(full, 'notifications/initialized')).status, 202);

  // The tools that tell an agent about its own connection, each taking nothing.
  const listed = (await rpc(full, 'tools/list')).body.result.tools as {
    name: string;
    inputSchema: { type: string; properties: Record<string, unknown> };
    annotations: { readOnlyHint: boolean };
  }[];
  assert.deepEqual(
    listed.map((tool) => tool.name),
    ['get_profile', 'whoami'],
  );
  for (const tool of listed) {
    assert.equal(tool.inputSchema.type, 'object');
    assert.deepEqual(tool.inputSchema.properties, {});
    assert.equal(tool.annotations.readOnlyHint, true);
  }

  // Current clients receive machine-readable structured data plus the complete text fallback:
  // some hosts negotiate the modern protocol but still expose only text to their model. Older
  // protocol clients keep the same compact JSON text they already consume.
  const who = await call(full, 'whoami', {});
  assert.equal(who.isError, false);
  assert.deepEqual(JSON.parse(who.content[0].text), who.structuredContent);
  assert.equal(who.structuredContent.key.name, 'Laptop – Claude Code');
  assert.deepEqual(
    who.structuredContent.dsps.map((dsp: { name: string }) => dsp.name),
    ['Northline Logistics'],
  );
  const legacy = await call(full, 'whoami', {}, '2025-03-26');
  assert.equal(legacy.structuredContent, undefined);
  assert.equal(JSON.parse(legacy.content[0].text).key.name, 'Laptop – Claude Code');
  const profile = await call(full, 'get_profile', {});
  assert.notEqual(profile.structuredContent.id, owner.session.user.id);
  assert.match(profile.structuredContent.id, /^profile_[A-Za-z0-9_-]+$/);
  assert.deepEqual(JSON.parse(profile.content[0].text), profile.structuredContent);

  // Refusals come back as answers the model reads, with what to fix and the choices; a name
  // too long to be a tool isn't repeated back.
  const unknown = await call(full, 'driver_report', {});
  assert.equal(unknown.isError, true);
  assert.match(unknown.content[0].text, /^unknown_tool: .*\nChoices: get_profile; whoami$/s);
  const longName = 'x'.repeat(25_000);
  const long = await call(full, longName, {});
  assert.equal(long.isError, true);
  assert.ok(!long.content[0].text.includes(longName));
  const extra = await call(full, 'whoami', { dsp: 'Northline Logistics' });
  assert.match(extra.content[0].text, /^invalid_input: unknown field `dsp`/);

  // 2026-07-28 drops the handshake: each request says who it is and what it speaks.
  const meta = {
    'io.modelcontextprotocol/protocolVersion': '2026-07-28',
    'io.modelcontextprotocol/clientInfo': { name: 'codex', version: '0.157.1' },
    'io.modelcontextprotocol/clientCapabilities': {},
  };
  const modern = { 'mcp-protocol-version': '2026-07-28' };
  const discovered = await rpc(
    full,
    'server/discover',
    { _meta: meta },
    {
      ...modern,
      'mcp-method': 'server/discover',
    },
  );
  assert.equal(discovered.status, 200, JSON.stringify(discovered.body));
  assert.ok(discovered.body.result.supportedVersions.includes('2026-07-28'));
  assert.ok(discovered.body.result.supportedVersions.includes('2025-03-26'));
  // Its tools list says how long it keeps, as its clients check before using it; the tools
  // follow the key, so only the key's own client may keep them.
  const modernTools = await rpc(
    full,
    'tools/list',
    { _meta: meta },
    {
      ...modern,
      'mcp-method': 'tools/list',
    },
  );
  assert.equal(modernTools.status, 200, JSON.stringify(modernTools.body));
  assert.equal(modernTools.body.result.cacheScope, 'private');
  assert.equal(typeof modernTools.body.result.ttlMs, 'number');
  const structuredTools = await rpc(
    full,
    'tools/list',
    {},
    { 'mcp-protocol-version': '2025-06-18' },
  );
  assert.deepEqual(modernTools.body.result.tools, structuredTools.body.result.tools);
  const stateless = await rpc(
    full,
    'tools/call',
    { name: 'whoami', arguments: {}, _meta: meta },
    { ...modern, 'mcp-method': 'tools/call', 'mcp-name': 'whoami' },
  );
  assert.equal(stateless.status, 200, JSON.stringify(stateless.body));
  assert.equal(stateless.body.result.structuredContent.key.name, 'Laptop – Claude Code');

  // Without a key, a client is told to bring one; a browser is refused outright.
  const keyless = await rpc(null, 'tools/list');
  assert.equal(keyless.status, 401);
  assert.equal(keyless.body.error, 'agent_key_required');
  assert.match(keyless.headers.get('www-authenticate') ?? '', /^Bearer realm="Dispatch"/);
  const browser = await rpc(full, 'tools/list', {}, { origin: 'https://evil.example' });
  assert.equal(browser.status, 403);
  const session = await rpc(full, 'tools/list', {}, { cookie: owner.headers.cookie! });
  assert.deepEqual([session.status, session.body.error], [400, 'session_and_key']);
  // No server-sent stream to open: answered the way the protocol asks.
  const stream = await fetch(`${server}/api/v1/mcp`, {
    headers: { authorization: `Bearer ${full}`, accept: 'text/event-stream' },
  });
  assert.equal(stream.status, 405);
});
