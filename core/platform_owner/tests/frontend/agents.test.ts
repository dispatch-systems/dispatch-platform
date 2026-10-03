import test from 'node:test';
import assert from 'node:assert/strict';
import type { AgentKey } from '../../shared/contracts/index.js';
import {
  accessText,
  activityNote,
  agentAreas,
  appKindName,
  areaGroups,
  areaLabels,
  areaSources,
  blankKey,
  bypassHere,
  canonicalAreas,
  connectApps,
  connectedAs,
  daysLeft,
  expiryOf,
  expiryText,
  inUse,
  keyState,
  knownApp,
  lastUsedText,
  reachText,
  reachedReads,
  requestOf,
  sameRequest,
  setups,
  signIns,
  surfaceOf,
  switchedOffText,
  withArea,
} from '../../dashboard/src/lib/agents.js';
import { changeText } from '../../dashboard/src/features/audit/wording.js';

const now = Date.parse('2026-10-01T15:00:00');
const DAY = 86_400_000;
const key = (change: Partial<AgentKey> = {}): AgentKey => ({
  id: 'agentkey_1',
  kind: 'key',
  client: null,
  name: 'Laptop – Claude Code',
  hint: 'x7Qp',
  access: 'read',
  reads: { areas: [...agentAreas], bypass: false },
  dspReads: [],
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
    ['Claude Code', 'Codex CLI', 'Hermes'],
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
  assert.equal(ways.next, 'Approve it on the Dispatch page that opens');
  assert.equal(claude!.next, 'Approve it on the Dispatch page that opens');
  assert.equal(codex!.next, 'Approve it on the Dispatch page that opens');
  assert.equal(
    hermes!.next,
    'Approve it on the Dispatch page that opens, then answer Y to turn on the tools',
  );

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

  // Any other app takes the address, or runs it through a local bridge.
  assert.equal(ways.bridge, `npx -y mcp-remote ${mcp}`);
});

test('Connect an app offers the known apps first, and knows each by the name it signs in with', () => {
  assert.deepEqual(
    connectApps.map((app) => app.label),
    ['ChatGPT', 'Codex CLI', 'Claude Code', 'Hermes', 'Other app'],
  );
  const client = (name: string, known = true) => ({
    name,
    known,
    status: 'connected' as const,
  });
  assert.equal(knownApp(client('Claude Code')), 'claude-code');
  assert.equal(knownApp(client('Codex')), 'codex');
  assert.equal(knownApp(client('Hermes Agent', false)), 'hermes');
  assert.equal(knownApp(client('ChatGPT')), 'chatgpt');
  assert.equal(knownApp(client('Cursor', false)), null);
  assert.equal(knownApp(null), null);

  // A new connection is the app being connected when it is that app; any app is "Other app".
  const app = (name: string) => key({ kind: 'app', client: client(name) });
  assert.ok(connectedAs(app('Claude Code'), 'claude-code'));
  assert.ok(!connectedAs(app('Claude Code'), 'codex'));
  assert.ok(!connectedAs(app('Cursor'), 'claude-code'));
  assert.ok(connectedAs(app('Cursor'), 'other'));
  assert.ok(connectedAs(app('Claude Code'), 'other'));
  // A key is never an app connecting.
  assert.ok(!connectedAs(key(), 'other'));
});

test('a key sheet knows when nothing changed', () => {
  const start = requestOf(
    key({
      allDsps: false,
      dsps: ['dsp_b', 'dsp_a'],
      dspReads: [
        { dsp: 'dsp_b', areas: ['dvic', 'routes'], bypass: true },
        { dsp: 'dsp_a', areas: [], bypass: false },
      ],
    }),
  );
  assert.ok(sameRequest(start, { ...start, dsps: ['dsp_a', 'dsp_b'] }));
  assert.ok(!sameRequest(start, { ...start, access: 'operator' }));
  // The DSPs' own settings in any order, and their kinds of data in any order, are the same.
  assert.ok(
    sameRequest(start, {
      ...start,
      dspReads: [
        { dsp: 'dsp_a', areas: [], bypass: false },
        { dsp: 'dsp_b', areas: ['routes', 'dvic', 'dvic'], bypass: true },
      ],
    }),
  );
  assert.ok(!sameRequest(start, { ...start, reads: { ...start.reads, bypass: true } }));
  assert.ok(!sameRequest(start, { ...start, dspReads: start.dspReads.slice(1) }));
  assert.equal(blankKey().access, 'read');
  assert.equal(blankKey().allDsps, true);
  // A new key or app reads everything but addresses and GPS, through no switched-off feature,
  // and no DSP has settings of its own.
  assert.deepEqual(blankKey().reads, {
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
  });
  assert.deepEqual(blankKey().dspReads, []);
});

test('a key keeps the own settings of the DSPs it still reaches, and only theirs', () => {
  const dspReads: AgentKey['dspReads'] = [
    { dsp: 'dsp_a', areas: [], bypass: false },
    { dsp: 'dsp_b', areas: [], bypass: true },
    // A suspended or removed DSP, which the page doesn't list.
    { dsp: 'dsp_away', areas: ['dvic'], bypass: true },
  ];
  assert.deepEqual(
    reachedReads({ allDsps: true, dsps: [], dspReads }).map((own) => own.dsp),
    ['dsp_a', 'dsp_b', 'dsp_away'],
  );
  assert.deepEqual(
    reachedReads({ allDsps: false, dsps: ['dsp_b', 'dsp_away'], dspReads }).map((own) => own.dsp),
    ['dsp_b', 'dsp_away'],
  );
  assert.deepEqual(
    reachedReads({ allDsps: false, dsps: ['dsp_b'], dspReads }).map((own) => own.dsp),
    ['dsp_b'],
  );
  assert.deepEqual(reachedReads({ allDsps: false, dsps: [], dspReads }), []);

  // A key reaching a DSP the page doesn't list opens unchanged, as the sheet compares it, and
  // sends that DSP's own settings back as they were.
  for (const reach of [
    { allDsps: true, dsps: [] },
    { allDsps: false, dsps: ['dsp_away', 'dsp_b'] },
  ]) {
    const start = requestOf(key({ ...reach, dspReads: dspReads.slice(1) }));
    const sent = { ...start, dspReads: reachedReads(start) };
    assert.ok(sameRequest(sent, start));
    assert.deepEqual(
      sent.dspReads.find((own) => own.dsp === 'dsp_away'),
      { dsp: 'dsp_away', areas: ['dvic'], bypass: true },
    );
  }
});

test('the kinds of data come in one order, grouped under the feature that collects them', () => {
  assert.deepEqual(agentAreas, [
    'routes',
    'locations',
    'timecards',
    'meal_breaks',
    'dvic',
    'feedback',
    'safety',
    'returns',
    'scorecard',
  ]);
  // Every kind once, in its group, in the same order.
  assert.deepEqual(
    areaGroups.flatMap((group) => group.areas),
    agentAreas,
  );
  assert.deepEqual(
    areaGroups.map((group) => group.label),
    ['Routes', 'Timecard', 'DVIC', 'Scorecard'],
  );
  assert.deepEqual(
    agentAreas.map((area) => areaLabels[area]),
    [
      'Routes & packages',
      'Delivery addresses & GPS',
      'Timecards',
      'Meal breaks',
      'DVIC inspections',
      'Customer feedback',
      'Safety events',
      'Returns & contact compliance',
      'Weekly scorecard',
    ],
  );
  assert.deepEqual(
    agentAreas.map((area) => areaSources[area]),
    [
      'routes',
      'routes',
      'timecards',
      'meal_breaks',
      'dvic',
      'scorecard',
      'scorecard',
      'scorecard',
      'scorecard',
    ],
  );
  // Switching keeps the order; addresses and GPS go with routes and need them back on.
  assert.deepEqual(withArea(['dvic', 'routes'], 'timecards', true), [
    'routes',
    'timecards',
    'dvic',
  ]);
  assert.deepEqual(withArea(['routes', 'locations', 'dvic'], 'routes', false), ['dvic']);
  assert.deepEqual(withArea(['dvic'], 'locations', true), ['dvic']);
  assert.deepEqual(withArea(['routes'], 'locations', true), ['routes', 'locations']);
  assert.deepEqual(canonicalAreas(['scorecard', 'scorecard', 'routes']), ['routes', 'scorecard']);
});

test('a row says how much a key or app reads, and what to know about it', () => {
  const dsps = [
    { id: 'dsp_a', name: 'Summit Delivery' },
    { id: 'dsp_b', name: 'Northline Logistics' },
  ];
  const reads = (areas: readonly (typeof agentAreas)[number][], bypass = false) =>
    key({ reads: { areas: [...areas], bypass } });
  const without = (...missing: string[]) => agentAreas.filter((area) => !missing.includes(area));
  assert.deepEqual(accessText(reads(agentAreas), dsps), {
    count: 'All data',
    note: '',
    bypass: false,
  });
  // One or two things it doesn't read are named; a whole group by what it holds.
  assert.deepEqual(accessText(reads(without('locations')), dsps), {
    count: '8 of 9 kinds',
    note: 'No delivery addresses',
    bypass: false,
  });
  assert.deepEqual(accessText(reads(without('feedback', 'safety', 'returns', 'scorecard')), dsps), {
    count: '5 of 9 kinds',
    note: 'No scorecard data',
    bypass: false,
  });
  assert.equal(
    accessText(reads(without('routes', 'locations', 'meal_breaks')), dsps).note,
    'No route data or meal breaks',
  );
  assert.equal(accessText(reads(without('dvic', 'safety', 'returns')), dsps).note, '');
  assert.deepEqual(accessText(reads([]), dsps), { count: '0 of 9 kinds', note: '', bypass: false });
  // Bypassing features says so, in place of what it misses.
  assert.deepEqual(accessText(reads(without('locations'), true), dsps), {
    count: '8 of 9 kinds',
    note: 'Bypass on',
    bypass: true,
  });
  // DSPs with settings of their own come first: one by name, more by count.
  const own = (dsp: string, bypass: boolean) => ({ dsp, areas: ['routes' as const], bypass });
  assert.deepEqual(accessText(key({ dspReads: [own('dsp_a', true)] }), dsps), {
    count: 'All data',
    note: 'Summit Delivery: own settings, bypass on',
    bypass: true,
  });
  assert.deepEqual(accessText(key({ dspReads: [own('dsp_b', false)] }), dsps), {
    count: 'All data',
    note: 'Northline Logistics: own settings',
    bypass: false,
  });
  assert.deepEqual(
    accessText(
      key({ reads: { areas: [...agentAreas], bypass: true }, dspReads: [own('dsp_a', false)] }),
      dsps,
    ),
    { count: 'All data', note: 'Summit Delivery: own settings', bypass: true },
  );
  assert.deepEqual(accessText(key({ dspReads: [own('dsp_a', false), own('dsp_b', true)] }), dsps), {
    count: 'All data',
    note: '2 DSPs with own settings',
    bypass: true,
  });
});

test('a DSP’s settings name the features it has switched off, and what bypassing them reads', () => {
  const dsp = (
    ...switchedOff: ('routes' | 'timecards' | 'meal_breaks' | 'dvic' | 'scorecard')[]
  ) => ({
    name: 'Summit Delivery',
    switchedOff,
  });
  assert.equal(switchedOffText(dsp()), 'Every feature is on at Summit Delivery.');
  assert.equal(switchedOffText(dsp('dvic')), 'DVIC is switched off at Summit Delivery.');
  // A page switched off takes its tab with it, and is named once.
  assert.equal(
    switchedOffText(dsp('routes', 'timecards', 'meal_breaks', 'scorecard')),
    'Routes, Timecard and Scorecard are switched off at Summit Delivery.',
  );
  assert.equal(
    switchedOffText(dsp('meal_breaks', 'dvic')),
    'Meal Breaks and DVIC are switched off at Summit Delivery.',
  );
  assert.equal(
    bypassHere('ChatGPT', dsp('routes', 'timecards', 'meal_breaks', 'scorecard')),
    'ChatGPT reads every feature’s data here, including the three switched off. Their ' +
      'collection stays off, so that data ends on the day each was switched off. It only ever reads.',
  );
  assert.equal(
    bypassHere('ChatGPT', dsp('dvic')),
    'ChatGPT reads every feature’s data here, including the one switched off. Its collection ' +
      'stays off, so that data ends on the day it was switched off. It only ever reads.',
  );
  assert.equal(
    bypassHere('ChatGPT', dsp()),
    'ChatGPT reads every feature’s data here, even if one is switched off later. It only ever reads.',
  );
});

test('the audit log names what a key or app reads, and still reads older key changes', () => {
  const change = (field: string, from: string | null, to: string | null) =>
    changeText({ field, from, to });
  assert.equal(change('reads.locations', 'false', 'true'), 'Delivery addresses & GPS Off → On');
  assert.equal(change('reads.meal_breaks', 'true', 'false'), 'Meal breaks On → Off');
  assert.equal(change('bypass', 'false', 'true'), 'Bypass features Off → On');
  // A DSP's own settings read as the server wrote them.
  assert.equal(
    change('dsp_reads', 'none', 'Summit Delivery: 7 of 9, bypass on; Harbor Route Co: 9 of 9'),
    'DSP settings none → Summit Delivery: 7 of 9, bypass on; Harbor Route Co: 9 of 9',
  );
  assert.equal(change('tools', 'full', 'essential'), 'Tools Full → Essential');
  assert.equal(change('locations', 'false', 'true'), 'Addresses and GPS Off → On');
});

test('a call names the endpoint or tool it reached, and how', () => {
  assert.deepEqual(surfaceOf('rest:whoami'), { via: 'REST', name: 'whoami' });
  assert.deepEqual(surfaceOf('mcp:find_driver'), { via: 'MCP', name: 'find_driver' });
  assert.deepEqual(surfaceOf('mcp:unknown'), { via: 'MCP', name: 'unknown' });
  // Anything else reads as it was recorded.
  assert.deepEqual(surfaceOf('openapi'), { via: '', name: 'openapi' });
  assert.deepEqual(surfaceOf('ws:stream'), { via: '', name: 'ws:stream' });
});

test('a kind of app the owner turned off is named in a sentence, or is this app', () => {
  assert.equal(appKindName('chatgpt'), 'ChatGPT');
  assert.equal(appKindName('codex'), 'Codex');
  assert.equal(appKindName('claude-code'), 'Claude Code');
  assert.equal(appKindName('hermes'), 'Hermes Agent');
  assert.equal(appKindName('local'), 'apps on this computer');
  assert.equal(appKindName('web'), 'websites and other apps');
  assert.equal(appKindName(null), 'this app');
  assert.equal(appKindName(''), 'this app');
  assert.equal(appKindName('cursor'), 'this app');
  assert.equal(appKindName('toString'), 'this app');
});

test('a key or app past 10,000 calls a UTC day is a note in the Activity log, not a call', () => {
  const now = Date.parse('2026-10-02T15:00:00Z');
  const marker = {
    at: '2026-10-02T14:12:09Z',
    outcome: 'capped',
    key: { id: 'agentkey_1', name: 'Nightly report script', kind: 'key' as const },
  };
  // The day is always named, as UTC counts it, since the cap starts again at UTC midnight.
  assert.equal(
    activityNote(marker, now),
    'Over 10,000 calls from Nightly report script on Oct 2 (UTC); later calls that day aren’t listed.',
  );
  // Early UTC on Oct 2 is still Oct 1 in Chicago, but the cap counts Oct 2.
  assert.equal(
    activityNote({ ...marker, at: '2026-10-02T03:00:00Z' }, now),
    'Over 10,000 calls from Nightly report script on Oct 2 (UTC); later calls that day aren’t listed.',
  );
  assert.equal(
    activityNote({ ...marker, at: '2026-10-01T23:59:00Z' }, now),
    'Over 10,000 calls from Nightly report script on Oct 1 (UTC); later calls that day aren’t listed.',
  );
  // Another year names its year.
  assert.equal(
    activityNote({ ...marker, at: '2025-12-31T23:30:00Z' }, now),
    'Over 10,000 calls from Nightly report script on Dec 31, 2025 (UTC); later calls that day aren’t listed.',
  );
  for (const outcome of ['ok', 'rate_limited'])
    assert.equal(activityNote({ ...marker, outcome }, now), null);
});
