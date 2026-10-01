import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../support/support.js';
import type { AgentKeyCreated, AgentKeys } from '../../shared/contracts/index.js';

type Rpc = { status: number; headers: Headers; body: any };

test('any MCP client reaches the agent API with a key, on every protocol version', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const { dsps }: AgentKeys = (await owner.get('/api/platform/agents')).value;
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const key = async (name: string, tools: 'full' | 'essential') => {
    const made = await owner.post('/api/platform/agents/keys', {
      name,
      allDsps: false,
      dsps: [north.id],
      access: 'read',
      tools,
      locations: false,
      expiresAt: null,
    });
    assert.equal(made.status, 200, made.body);
    return (made.value as AgentKeyCreated).token;
  };
  // The server listens on loopback and names itself by its origin, as behind a tunnel.
  const server = `http://127.0.0.1:${f.env.PORT}`;
  const origin = f.env.DISPATCH_ORIGIN!;
  const full = await key('Laptop – Claude Code', 'full');
  const essential = await key('Home server – Hermes', 'essential');
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
  assert.match(hello.body.result.instructions, /Call whoami first/);
  assert.ok(hello.body.result.capabilities.tools && hello.body.result.capabilities.prompts);
  assert.equal((await rpc(full, 'notifications/initialized')).status, 202);

  // Full keys get every tool, Essential keys the few small models choose between best.
  const listed = (await rpc(full, 'tools/list')).body.result.tools as {
    name: string;
    inputSchema: { type: string; properties: Record<string, { type: string }> };
    annotations: { readOnlyHint: boolean };
  }[];
  assert.ok(listed.length >= 12);
  for (const tool of listed) {
    assert.match(tool.name, /^[a-z_]+$/);
    assert.equal(tool.inputSchema.type, 'object');
    assert.equal(tool.annotations.readOnlyHint, true);
    // Flat inputs: no nested objects or arrays for a model to get wrong.
    for (const property of Object.values(tool.inputSchema.properties))
      assert.ok(['string', 'integer', 'boolean'].includes(property.type), tool.name);
  }
  const few = (await rpc(essential, 'tools/list')).body.result.tools as { name: string }[];
  assert.deepEqual(few.map((tool) => tool.name).sort(), [
    'data_status',
    'driver_report',
    'find_drivers',
    'route_day',
    'team_table',
    'whoami',
  ]);
  const missing = await call(essential, 'timecards', {});
  assert.equal(missing.isError, true);
  assert.match(missing.content[0].text, /^unknown_tool/);

  // Every answer is data and the same data as text, for harnesses that show only text.
  const found = await call(full, 'find_drivers', { q: 'a', limit: 5 });
  assert.equal(found.isError, false);
  assert.deepEqual(JSON.parse(found.content[0].text), found.structuredContent);
  assert.equal(found.structuredContent.dsp.name, 'Northline Logistics');
  const someone = found.structuredContent.drivers[0] as { code: string; name: string };
  const report = await call(full, 'driver_report', { driver: someone.name, period: 'last 7 days' });
  assert.equal(report.structuredContent.driver.code, someone.code);
  assert.equal(report.structuredContent.period.days, 7);
  // A list where a comma-separated value belongs, as some models send, still works.
  const table = await call(full, 'team_table', {
    metrics: ['hours_worked', 'days_worked'],
    period: 'last 7 days',
  });
  assert.equal(table.isError, false, table.content[0].text);
  assert.deepEqual(
    table.structuredContent.metrics.map((m: { name: string }) => m.name),
    ['hours_worked', 'days_worked'],
  );

  // Refusals come back as answers the model reads, with what to fix and the choices.
  const unknown = await call(full, 'team_table', { metrics: 'steps' });
  assert.equal(unknown.isError, true);
  assert.match(unknown.content[0].text, /^unknown_metric: .*\nChoices: .*hours_worked/s);
  const nobody = await call(full, 'driver_report', { driver: 'Nobody Anywhere' });
  assert.match(nobody.content[0].text, /^driver_not_found/);
  const bare = await call(full, 'driver_report', {});
  assert.match(bare.content[0].text, /^missing_parameter/);

  // Ready-made requests a harness can offer as commands.
  const prompts = (await rpc(full, 'prompts/list')).body.result.prompts as { name: string }[];
  assert.deepEqual(
    prompts.map((p) => p.name),
    ['daily_summary', 'driver_review'],
  );
  const review = await rpc(full, 'prompts/get', {
    name: 'driver_review',
    arguments: { driver: 'Avery Morgan' },
  });
  assert.match(review.body.result.messages[0].content.text, /review Avery Morgan for last week/);

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
  // Its lists say how long they keep, as its clients check before using them; the tools
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
  assert.equal(modernTools.body.result.tools.length, listed.length);
  const modernPrompts = await rpc(
    full,
    'prompts/list',
    { _meta: meta },
    {
      ...modern,
      'mcp-method': 'prompts/list',
    },
  );
  assert.equal(typeof modernPrompts.body.result.ttlMs, 'number');
  assert.equal(modernPrompts.body.result.cacheScope, 'public');
  const stateless = await rpc(
    full,
    'tools/call',
    { name: 'whoami', arguments: {}, _meta: meta },
    { ...modern, 'mcp-method': 'tools/call', 'mcp-name': 'whoami' },
  );
  assert.equal(stateless.status, 200, JSON.stringify(stateless.body));
  assert.equal(stateless.body.result.structuredContent.key.tools, 'full');

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

  // The skill, for harnesses that load one, names this server.
  const skill = await fetch(`${server}/api/v1/skill`, {
    headers: { authorization: `Bearer ${full}` },
  });
  assert.match(skill.headers.get('content-type') ?? '', /^text\/markdown/);
  const text = await skill.text();
  assert.match(text, /^---\nname: dispatch\n/);
  assert.ok(text.includes(`${origin}/api/v1/mcp`));
  // The Agents page downloads the same file.
  const download = await fetch(`${server}/api/platform/agents/skill`, {
    headers: { cookie: owner.headers.cookie! },
  });
  assert.equal(download.headers.get('content-disposition'), 'attachment; filename="SKILL.md"');
  assert.equal(await download.text(), text);
});
