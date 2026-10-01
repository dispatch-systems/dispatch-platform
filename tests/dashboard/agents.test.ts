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
  setupSnippet,
} from '../../dashboard/src/lib/agents.js';

const now = Date.parse('2026-10-01T15:00:00');
const DAY = 86_400_000;
const key = (change: Partial<AgentKey> = {}): AgentKey => ({
  id: 'agentkey_1',
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
  const snippet = setupSnippet('https://dispatch.example.com', 'dsk_dev_abc');
  assert.ok(snippet.includes('export DISPATCH_KEY="dsk_dev_abc"'));
  assert.ok(snippet.includes('https://dispatch.example.com/api/v1/whoami'));
});

test('a key sheet knows when nothing changed', () => {
  const start = requestOf(key({ allDsps: false, dsps: ['dsp_b', 'dsp_a'] }));
  assert.ok(sameRequest(start, { ...start, dsps: ['dsp_a', 'dsp_b'] }));
  assert.ok(!sameRequest(start, { ...start, access: 'operator' }));
  assert.equal(blankKey().access, 'read');
  assert.equal(blankKey().allDsps, true);
});
