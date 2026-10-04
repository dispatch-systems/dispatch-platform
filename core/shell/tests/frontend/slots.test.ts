import test from 'node:test';
import assert from 'node:assert/strict';
import { Award, Shirt } from 'lucide-react';
import { dspHash, parseHash, rememberDestination } from '../../frontend/runtime/navigation.js';
import {
  collectionAffects,
  collectionData,
  mutationAffects,
} from '../../frontend/runtime/data-policy.js';
import {
  cacheRules,
  capabilityLabelOf,
  collectionLabels,
  connectionCard,
  connectionCards,
  errorLabelOf,
  installFeatures,
  isLongPoll,
  pageTabs,
  readToggles,
  scheduleIssueOf,
  auditWording,
  loadPlatformSlots,
  switchIcon,
  type CollectionLabels,
  type ConnectionCard,
  type DspRoute,
  type PageTab,
  type PlatformSlots,
  type ReadToggles,
} from '../../frontend/runtime/slots.js';

// Synthetic owners, installed as the app installs its manifests. A platform-slots module's
// loader resolves to the module.
const loads = (slots: PlatformSlots) => async () => ({ slots });
const group = (label: string, order: number): ReadToggles => ({
  label,
  missing: `${label.toLowerCase()} data`,
  order,
  sources: {},
  toggles: [],
});

test('read toggles come group by group in their order, ties in the order the owners are listed', async () => {
  installFeatures([
    { name: 'alpha', platformSlots: loads({ readToggles: group('Alpha', 20) }) },
    { name: 'beta' },
    { name: 'gamma', platformSlots: loads({ readToggles: group('Gamma', 10) }) },
    { name: 'delta', platformSlots: loads({ readToggles: group('Delta', 20) }) },
  ]);
  await loadPlatformSlots();
  assert.deepEqual(
    readToggles().map((each) => each.label),
    ['Gamma', 'Alpha', 'Delta'],
  );
});

test("a page's switch shows the icon its feature declares, once loaded", async () => {
  installFeatures([
    { name: 'alpha', platformSlots: loads({ switch: { id: 'uniforms', icon: Shirt } }) },
    { name: 'beta' },
    { name: 'gamma', platformSlots: loads({ switch: { id: 'scorecard', icon: Award } }) },
  ]);
  assert.equal(switchIcon('uniforms'), undefined);
  await loadPlatformSlots();
  assert.equal(switchIcon('uniforms'), Shirt);
  assert.equal(switchIcon('scorecard'), Award);
  assert.equal(switchIcon('timecard'), undefined);
});

test("the platform owner's slots load once, in the order the owners are listed, and again after a failure", async () => {
  let attempts = 0;
  let fail = true;
  installFeatures([
    { name: 'alpha', platformSlots: loads({ auditWording: { spoken: ['alpha'] } }) },
    {
      name: 'beta',
      platformSlots: async () => {
        attempts++;
        if (fail) throw new Error('offline');
        return { slots: { auditWording: { spoken: ['beta'] } } };
      },
    },
  ]);
  await assert.rejects(loadPlatformSlots(), /offline/);
  assert.deepEqual(auditWording(), []);
  fail = false;
  await loadPlatformSlots();
  await loadPlatformSlots();
  assert.equal(attempts, 2);
  assert.deepEqual(
    auditWording().map((wording) => wording.spoken),
    [['alpha'], ['beta']],
  );
});

test('connection cards come in the order their collectors are listed', () => {
  const card = (provider: ConnectionCard['provider']): ConnectionCard => ({
    provider,
    read: `/api/dsp/connections/${provider}`,
    load: () => Promise.resolve(),
    render: () => provider,
  });
  installFeatures([
    { name: 'gamma', connectionCard: card('cortex') },
    { name: 'beta' },
    { name: 'alpha', connectionCard: card('paycom') },
  ]);
  assert.deepEqual(
    connectionCards().map((each) => each.provider),
    ['cortex', 'paycom'],
  );
  assert.equal(connectionCard('paycom')?.read, '/api/dsp/connections/paycom');
  assert.equal(connectionCard('other'), undefined);
});

test('collections come with the collector that runs them, in the order they are declared', async () => {
  const collection = (kind: string): CollectionLabels => ({
    kind,
    schedule: { id: kind, label: kind },
    unit: 'item',
    count: (metrics) => metrics.rows,
  });
  installFeatures([
    {
      name: 'beta',
      platformSlots: loads({ collections: [collection('beta.b'), collection('beta.a')] }),
    },
    { name: 'gamma' },
    { name: 'alpha', platformSlots: loads({ collections: [collection('alpha.a')] }) },
  ]);
  await loadPlatformSlots();
  assert.deepEqual(
    collectionLabels().map(({ provider, kind }) => [provider, kind]),
    [
      ['beta', 'beta.b'],
      ['beta', 'beta.a'],
      ['alpha', 'alpha.a'],
    ],
  );
});

test('cache rules keep each owner’s reads current, a read changing when any owner says so', () => {
  installFeatures([
    {
      name: 'alpha',
      cache: {
        collected: ['/api/dsp/alpha/days', '/api/dsp/alpha/status'],
        collection: (url, changes) =>
          url.startsWith('/api/dsp/alpha/status') || changes.some((change) => change.roster),
        jobs: ['/api/dsp/alpha/status'],
        write: (write, url) =>
          write.startsWith('/api/dsp/alpha/') ? url.startsWith('/api/dsp/alpha/') : undefined,
      },
    },
    { name: 'beta' },
    {
      name: 'gamma',
      cache: {
        collected: ['/api/dsp/gamma'],
        connections: ['/api/dsp/gamma/'],
        // Alpha's writes move gamma's rows too.
        write: (write, url) =>
          write.startsWith('/api/dsp/alpha/') ? url.startsWith('/api/dsp/gamma/rows') : undefined,
      },
    },
  ]);
  assert.equal(cacheRules().length, 2);
  const change = { provider: 'fixture', dates: [], employeeCode: null, roster: false };
  assert.equal(collectionData('/api/dsp/alpha/days?date=2026-09-22'), true);
  assert.equal(collectionData('/api/dsp/delta'), false);
  assert.equal(collectionAffects('/api/dsp/alpha/days', [change]), false);
  assert.equal(collectionAffects('/api/dsp/alpha/days', [{ ...change, roster: true }]), true);
  assert.equal(collectionAffects('/api/dsp/alpha/status', []), true);
  assert.equal(collectionAffects('/api/dsp/gamma/rows', [change]), true);
  assert.equal(collectionAffects('/api/dsp/jobs', []), true);
  assert.equal(mutationAffects('/api/dsp/alpha/edit', '/api/dsp/alpha/days'), true);
  assert.equal(mutationAffects('/api/dsp/alpha/edit', '/api/dsp/gamma/rows'), true);
  assert.equal(mutationAffects('/api/dsp/alpha/edit', '/api/dsp/gamma/other'), false);
  // A write an owner knows changes nothing else, the jobs included.
  assert.equal(mutationAffects('/api/dsp/alpha/collect', '/api/dsp/jobs'), false);
  assert.equal(mutationAffects('/api/dsp/delta/collect', '/api/dsp/alpha/status'), true);
  assert.equal(mutationAffects('/api/dsp/delta/collect', '/api/dsp/gamma/rows'), false);
  assert.equal(mutationAffects('/api/dsp/connections/fixture', '/api/dsp/gamma/rows'), true);
  assert.equal(mutationAffects('/api/dsp/connections/fixture', '/api/dsp/alpha/status'), true);
  assert.equal(mutationAffects('/api/dsp/connections/fixture', '/api/dsp/alpha/days'), false);
});

test('an owner says what its error codes, schedule issues and long reads are', () => {
  installFeatures([
    { name: 'alpha', errors: { alpha_full: 'Alpha is full.' } },
    { name: 'beta', scheduleIssues: { beta_waits: 'Connect Beta first.' } },
    { name: 'gamma', longPolls: ['/api/dsp/gamma/updates'] },
  ]);
  assert.equal(errorLabelOf('alpha_full'), 'Alpha is full.');
  // A schedule's issue is also what its error says.
  assert.equal(errorLabelOf('beta_waits'), 'Connect Beta first.');
  assert.equal(scheduleIssueOf('beta_waits'), 'Connect Beta first.');
  assert.equal(scheduleIssueOf('alpha_full'), undefined);
  assert.equal(errorLabelOf('unknown'), undefined);
  assert.equal(isLongPoll('/api/dsp/gamma/updates?after=1'), true);
  assert.equal(isLongPoll('/api/dsp/gamma'), false);
});

test('a capability is named by the first connection listed that provides it', async () => {
  installFeatures([
    { name: 'alpha', platformSlots: loads({ capabilities: { photos: 'a photo source' } }) },
    {
      name: 'beta',
      platformSlots: loads({
        capabilities: { photos: 'another photo source', notes: 'a notes source' },
      }),
    },
  ]);
  await loadPlatformSlots();
  assert.equal(capabilityLabelOf('photos'), 'a photo source');
  assert.equal(capabilityLabelOf('notes'), 'a notes source');
  assert.equal(capabilityLabelOf('maps'), undefined);
});

const page = (id: DspRoute['id'], remembered?: boolean): DspRoute => ({
  id,
  scope: 'dsp',
  label: id,
  nav: true,
  preload: () => Promise.resolve(),
  render: () => null,
  ...(remembered === undefined ? {} : { remembered }),
});

test('opening a DSP again returns to the last page open, unless that page is not remembered', () => {
  installFeatures([
    {
      name: 'alpha',
      routes: [{ ...page('overview'), landing: true }, page('team'), page('settings', false)],
    },
  ]);
  assert.equal(dspHash('dsp_fixture'), '#dsp/dsp_fixture/overview');
  rememberDestination('dsp_fixture', 'team');
  assert.equal(dspHash('dsp_fixture'), '#dsp/dsp_fixture/team');
  rememberDestination('dsp_fixture', 'settings');
  assert.equal(dspHash('dsp_fixture'), '#dsp/dsp_fixture/team');
});

test('a DSP opens on the page a feature declares it lands on, and names none without one', () => {
  installFeatures([
    { name: 'alpha', routes: [page('team'), { ...page('overview'), landing: true }] },
  ]);
  assert.equal(dspHash('dsp_landing'), '#dsp/dsp_landing/overview');
  assert.equal(parseHash('#dsp/dsp_landing').page, 'overview');
  installFeatures([{ name: 'alpha', routes: [page('team')] }]);
  assert.equal(dspHash('dsp_landing'), '#dsp/dsp_landing/');
  assert.equal(parseHash('#dsp/dsp_landing').page, '');
  assert.equal(parseHash('#dsp/dsp_landing/team').page, 'team');
});

test("a page's tabs from other features come in their order, ties in the order they are listed", () => {
  const tab = (page: PageTab['page'], id: string, order: number): PageTab => ({
    page,
    id,
    label: id,
    order,
    load: () => Promise.resolve(),
    render: () => id,
  });
  installFeatures([
    { name: 'alpha', pageTabs: [tab('team', 'alpha-late', 30), tab('settings', 'elsewhere', 1)] },
    { name: 'beta', pageTabs: [tab('team', 'beta', 10)] },
    { name: 'gamma', pageTabs: [tab('team', 'gamma', 30)] },
  ]);
  assert.deepEqual(
    pageTabs('team').map((each) => each.id),
    ['beta', 'alpha-late', 'gamma'],
  );
  assert.deepEqual(pageTabs('uniforms'), []);
});
