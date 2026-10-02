import test from 'node:test';
import assert from 'node:assert/strict';
import type { AgentKey } from '../../shared/contracts/index.js';
import {
  blankKey,
  daysLeft,
  expiryOf,
  expiryText,
  inUse,
  keyState,
  lastUsedText,
  reachText,
  requestOf,
  sameRequest,
  setups,
  signIns,
} from '../../dashboard/src/lib/agents.js';

const now = Date.parse('2026-10-01T15:00:00');
const DAY = 86_400_000;
const key = (change: Partial<AgentKey> = {}): AgentKey => ({
  id: 'agentkey_1',
  kind: 'key',
  client: null,
  name: 'Laptop – Claude Code',
  hint: 'x7Qp',
  access: 'read',
  tools: 'full',
  locations: false,
  allDsps: true,
  dsps: [],
  createdAt: '2026-09-02T10:00:00Z',
  expiresAt: new Date(now + 90 * DAY).toISOString(),
  revokedAt: null,
  lastUsedAt: null,
  lastClient: null,
  ...change,
});

test('a key is in use, expiring within a week, expired, revoked or never ending', () => {
  assert.equal(keyState(key(), now), 'active');
  assert.equal(
    keyState(key({ expiresAt: new Date(now + 5 * DAY).toISOString() }), now),
    'expiring',
  );
  assert.equal(daysLeft(key({ expiresAt: new Date(now + 4.2 * DAY).toISOString() }), now), 5);
  assert.equal(keyState(key({ expiresAt: new Date(now - 1000).toISOString() }), now), 'expired');
  assert.equal(keyState(key({ expiresAt: null }), now), 'never');
  assert.equal(keyState(key({ revokedAt: '2026-09-14T12:00:00Z' }), now), 'revoked');
  assert.ok(inUse(key({ expiresAt: null }), now));
  assert.ok(!inUse(key({ revokedAt: '2026-09-14T12:00:00Z' }), now));
});

test('expiry reads as a date, a countdown, or never', () => {
  assert.equal(expiryText(key({ expiresAt: '2026-12-30T12:00:00Z' }), now), 'Dec 30, 2026');
  assert.equal(
    expiryText(key({ expiresAt: new Date(now + 5 * DAY).toISOString() }), now),
    'Oct 6 · 5 days',
  );
  assert.equal(expiryText(key({ expiresAt: null }), now), 'Never');
  assert.equal(expiryText(key({ revokedAt: '2026-09-14T12:00:00Z' }), now), 'Revoked Sep 14');
  // A preset counts days from now; a picked date lasts through that day.
  assert.equal(expiryOf('7', '', now), new Date(now + 7 * DAY).toISOString());
  assert.equal(expiryOf('never', '', now), null);
  assert.equal(expiryOf('date', '2026-11-03', now), new Date('2026-11-03T23:59:59').toISOString());
});

test('reach, last use and setup read plainly', () => {
  const dsps = [
    { id: 'dsp_a', name: 'Blue Heron Logistics' },
    { id: 'dsp_b', name: 'Northline Logistics' },
  ];
  assert.deepEqual(reachText(key(), dsps), { count: 'All DSPs', names: '' });
  assert.deepEqual(reachText(key({ allDsps: false, dsps: ['dsp_b', 'dsp_gone'] }), dsps), {
    count: '2 DSPs',
    names: 'Northline Logistics, A removed DSP',
  });
  assert.equal(lastUsedText(null, now), 'Never used');
  assert.equal(lastUsedText(new Date(now - 2 * 60_000).toISOString(), now), '2 min ago');
  assert.equal(lastUsedText(new Date(now - 3 * 3_600_000).toISOString(), now), 'Today, 12:00 PM');
  assert.match(lastUsedText(new Date(now - DAY).toISOString(), now), /^Yesterday, /);
  assert.equal(lastUsedText('2026-09-28T15:00:00', now), 'Sep 28');
  // Every agent gets its own setup, each naming the same MCP address and key.
  const ready = setups('https://dispatch.example.com', 'dsk_dev_abc');
  assert.deepEqual(
    ready.map((setup) => setup.label),
    ['Claude Code', 'Codex', 'Hermes', 'Other MCP apps', 'REST and curl'],
  );
  for (const setup of ready) assert.ok(setup.text.includes('dsk_dev_abc'), setup.id);
  for (const setup of ready.slice(0, 4))
    assert.ok(setup.text.includes('https://dispatch.example.com/api/v1/mcp'), setup.id);
  assert.match(ready[0].text, /claude mcp add --transport http --scope user dispatch/);
  assert.match(ready[1].text, /--bearer-token-env-var DISPATCH_KEY/);
  assert.match(ready[2].text, /Authorization: "Bearer \$\{DISPATCH_KEY\}"/);
  assert.deepEqual(JSON.parse(ready[3].text).mcpServers.dispatch.headers, {
    Authorization: 'Bearer dsk_dev_abc',
  });
  assert.ok(ready[4].text.includes('https://dispatch.example.com/api/v1/whoami'));
});

test('each app signs in with one command, a way from another machine and a prompt', () => {
  const mcp = 'https://dispatch.example.com/api/v1/mcp';
  const ways = signIns('https://dispatch.example.com');
  assert.equal(ways.mcp, mcp);
  const [claude, codex, hermes] = ways.terminals;
  assert.deepEqual(
    ways.terminals.map((app) => app.label),
    ['Claude Code', 'Codex', 'Hermes'],
  );
  assert.equal(
    claude!.command,
    `claude mcp add --transport http --scope user dispatch ${mcp} && claude mcp login dispatch`,
  );
  assert.equal(
    claude!.remote.command,
    `claude mcp add --transport http --scope user dispatch ${mcp} && claude mcp login dispatch --no-browser`,
  );
  assert.equal(codex!.command, `codex mcp add dispatch --url ${mcp}`);
  assert.equal(codex!.remote.command, 'codex mcp login dispatch --no-browser');
  assert.equal(
    hermes!.command,
    `hermes mcp add dispatch --url ${mcp} --auth oauth --connect-timeout 300`,
  );
  // Hermes asks for the address itself when it can't open a browser.
  assert.equal(hermes!.remote.command, undefined);
  for (const app of ways.terminals) assert.match(app.next, /^A Dispatch page opens — approve it/);

  // Each prompt asks the app to run the setup, say when to approve, and fall back to the link.
  assert.equal(
    claude!.prompt,
    [
      'Connect Claude Code to Dispatch for me.',
      `1. Run: claude mcp add --transport http --scope user dispatch ${mcp}`,
      "2. Sign in with `claude mcp login dispatch`. It needs a real terminal; if you can't give " +
        'it one (with script or tmux, say), ask me to run it in my own terminal, or /mcp in a ' +
        'new Claude Code session.',
      '3. It opens a Dispatch page in my browser: tell me to approve it there. ' +
        "If it can't open a browser here, use `claude mcp login dispatch --no-browser`, send me " +
        'the link it prints, and when I paste back the address my browser ends on, enter it at ' +
        'its prompt or open it with curl on this machine.',
      '4. Then tell me to restart Claude Code to load the Dispatch tools.',
    ].join('\n'),
  );
  assert.equal(
    codex!.prompt,
    [
      'Connect Codex to Dispatch for me.',
      `1. Run: codex mcp add dispatch --url ${mcp}`,
      '2. It opens a Dispatch page in my browser: tell me to approve it there. ' +
        "If it can't open a browser here, stop it and run `codex mcp login dispatch " +
        '--no-browser`, send me the link it prints, and when I paste back the address my ' +
        'browser ends on, enter it at its prompt or open it with curl on this machine.',
      '3. Then tell me to restart Codex to load the Dispatch tools.',
    ].join('\n'),
  );
  assert.equal(
    hermes!.prompt,
    [
      'Connect Hermes to Dispatch for me.',
      '1. Add this under mcp_servers in ~/.hermes/config.yaml, keeping everything else:',
      '  dispatch:',
      `    url: "${mcp}"`,
      '    auth: oauth',
      '2. Run `hermes mcp login dispatch`. It opens a Dispatch page in my browser: tell me to ' +
        "approve it there. If it can't open a browser here, send me the link it prints, and " +
        'when I paste back the address my browser ends on, enter it at its prompt or open it ' +
        'with curl on this machine.',
      '3. Then tell me to run /reload-mcp to load the Dispatch tools.',
    ].join('\n'),
  );

  // Any other app takes the address, as a JSON entry or through a local bridge.
  assert.deepEqual(JSON.parse(ways.json), { mcpServers: { dispatch: { url: mcp } } });
  assert.equal(ways.bridge, `npx -y mcp-remote ${mcp}`);
});

test('a key sheet knows when nothing changed', () => {
  const start = requestOf(key({ allDsps: false, dsps: ['dsp_b', 'dsp_a'] }));
  assert.ok(sameRequest(start, { ...start, dsps: ['dsp_a', 'dsp_b'] }));
  assert.ok(!sameRequest(start, { ...start, access: 'operator' }));
  assert.equal(blankKey().access, 'read');
  assert.equal(blankKey().allDsps, true);
});
