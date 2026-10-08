import test from 'node:test';
import assert from 'node:assert/strict';
import { Award, Shirt } from 'lucide-react';
import {
  dspHash,
  parseHash,
  rememberDestination,
  settingsHash,
} from '../../frontend/runtime/navigation.js';
import {
  collectionAffects,
  collectionData,
  mutationAffects,
} from '../../frontend/runtime/data-policy.js';
import {
  cacheRules,
  connectionCard,
  connectionCards,
  errorLabelOf,
  installFeatures,
  isLongPoll,
  pageTabs,
  runWording,
  settingsPieces,
  settingsTabs,
  scheduleIssueOf,
  auditWording,
  loadPlatformSlots,
  switchIcon,
  type ConnectionCard,
  type DspRoute,
  type DspRouteId,
  type PageTab,
  type PlatformSlots,
  type RunWording,
  type SettingsPiece,
  type SettingsTab,
} from '../../frontend/runtime/slots.js';
import { featureCatalog, type SubEntry } from '../../frontend/runtime/features.js';
import type { DspView } from '../../../accounts/api/index.js';
import type { Feature, PageFeature } from '../../../tenancy/api/index.js';

// Synthetic owners, installed as the app installs its manifests. A platform-slots module's
// loader resolves to the module. The pages and switches they declare are the test's own too,
// whatever features this build has.
const pageId = (id: string) => id as DspRouteId;
const switchId = (id: string) => id as PageFeature;
const loads = (slots: PlatformSlots) => async () => ({ slots });

test("a page's switch shows the icon its feature declares, once loaded", async () => {
  installFeatures([
    { name: 'alpha', platformSlots: loads({ switch: { id: switchId('uniforms'), icon: Shirt } }) },
    { name: 'beta' },
    {
      name: 'gamma',
      platformSlots: loads({ switch: { id: switchId('weekly_scorecard'), icon: Award } }),
    },
  ]);
  assert.equal(switchIcon('uniforms'), undefined);
  await loadPlatformSlots();
  assert.equal(switchIcon('uniforms'), Shirt);
  assert.equal(switchIcon('weekly_scorecard'), Award);
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

test("a collection's runs are worded by the first owner listed that words them, once loaded", async () => {
  const wording = (collected: string): RunWording => ({
    collected: () => collected,
    reads: () => [],
    slowReads: () => null,
  });
  installFeatures([
    { name: 'alpha' },
    { name: 'beta', platformSlots: loads({ runWording: wording('beta') }) },
    { name: 'gamma', platformSlots: loads({ runWording: wording('gamma') }) },
  ]);
  assert.equal(runWording(), undefined);
  await loadPlatformSlots();
  assert.equal(runWording()?.collected({} as never), 'beta');
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

const page = (id: string, remembered?: boolean): DspRoute => ({
  id: pageId(id),
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
  rememberDestination('dsp_fixture', pageId('team'));
  assert.equal(dspHash('dsp_fixture'), '#dsp/dsp_fixture/team');
  rememberDestination('dsp_fixture', pageId('settings'));
  assert.equal(dspHash('dsp_fixture'), '#dsp/dsp_fixture/team');
});

test('a DSP opens on the page a feature declares it lands on, and names none without one', () => {
  installFeatures([
    { name: 'alpha', routes: [page('board'), { ...page('lobby'), landing: true }] },
  ]);
  assert.equal(dspHash('dsp_landing'), '#dsp/dsp_landing/lobby');
  assert.equal(parseHash('#dsp/dsp_landing').page, 'lobby');
  installFeatures([{ name: 'alpha', routes: [page('board')] }]);
  assert.equal(dspHash('dsp_landing'), '#dsp/dsp_landing/');
  assert.equal(parseHash('#dsp/dsp_landing').page, '');
  assert.equal(parseHash('#dsp/dsp_landing/board').page, 'board');
});

test('a link to Settings opens the page that draws the settings tabs, and only on a tab it has', () => {
  const tab = (id: string): SettingsTab => ({
    id,
    label: id,
    order: 1,
    load: () => Promise.resolve(),
    render: () => id,
  });
  installFeatures([
    { name: 'alpha', routes: [page('board'), { ...page('options'), hostsSettings: true }] },
    { name: 'beta', settingsTabs: [tab('beta-panel')] },
  ]);
  assert.equal(settingsHash('dsp_settings'), '#dsp/dsp_settings/options');
  assert.equal(
    settingsHash('dsp_settings', 'beta-panel'),
    '#dsp/dsp_settings/options?tab=beta-panel',
  );
  assert.equal(settingsHash('dsp_settings', 'gone'), undefined);
  installFeatures([{ name: 'beta', routes: [page('board')], settingsTabs: [tab('beta-panel')] }]);
  assert.equal(settingsHash('dsp_settings'), undefined);
  assert.equal(settingsHash('dsp_settings', 'beta-panel'), undefined);
});

test("a page's tabs from other features come in their order, ties in the order they are listed", () => {
  const tab = (page: string, id: string, order: number): PageTab => ({
    page: pageId(page),
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
  const view = { features: [] } as unknown as DspView;
  assert.deepEqual(
    pageTabs('team', view).map((each) => each.id),
    ['beta', 'alpha-late', 'gamma'],
  );
  assert.deepEqual(pageTabs('uniforms', view), []);
});

// A feature's contributions to other pages go with it, whether switched off or hidden from the
// DSP, and a part's with the part; core's stay. The owner is one this build has, with a part.
test("what a feature adds to another's page goes with it, and a part's with the part", (t) => {
  const part = featureCatalog.find((f): f is SubEntry => f.kind === 'sub');
  if (!part) return t.skip('this build has no feature with parts');
  const owner = part.page;
  const view = (...features: Feature[]) => ({ features }) as unknown as DspView;
  const tab = (id: string, extra: Partial<SettingsTab> = {}): SettingsTab => ({
    id,
    label: id,
    order: 1,
    load: () => Promise.resolve(),
    render: () => id,
    ...extra,
  });
  const piece = (id: string, order: number, extra: Partial<SettingsPiece> = {}): SettingsPiece => ({
    tab: 'general',
    id,
    order,
    load: () => Promise.resolve(),
    render: () => id,
    ...extra,
  });
  const pageTab = (id: string, extra: Partial<PageTab> = {}): PageTab => ({
    page: pageId('team'),
    id,
    label: id,
    order: 1,
    load: () => Promise.resolve(),
    render: () => id,
    ...extra,
  });
  installFeatures([
    { name: 'core-owner', settingsTabs: [tab('general')], settingsPieces: [piece('core', 50)] },
    {
      name: owner,
      settingsTabs: [tab('own'), tab('of-part', { part: part.id })],
      settingsPieces: [
        piece('late', 30),
        piece('early', 10),
        piece('of-part', 20, { part: part.id }),
        piece('elsewhere', 1, { tab: 'own' }),
        piece('unseen', 1, { visible: () => false }),
      ],
      pageTabs: [pageTab('own'), pageTab('of-part', { part: part.id })],
    },
  ]);
  const ids = (each: readonly { id: string }[]) => each.map(({ id }) => id);
  // Every tab exists, so links to one keep working; a view lists only what it has.
  assert.deepEqual(ids(settingsTabs()), ['general', 'own', 'of-part']);
  assert.deepEqual(ids(settingsTabs(view(owner, part.id))), ['general', 'own', 'of-part']);
  assert.deepEqual(ids(settingsPieces('general', view(owner, part.id))), [
    'early',
    'of-part',
    'late',
    'core',
  ]);
  assert.deepEqual(ids(pageTabs('team', view(owner, part.id))), ['own', 'of-part']);
  // Without the part, its own go; without the feature, all of the feature's.
  assert.deepEqual(ids(settingsTabs(view(owner))), ['general', 'own']);
  assert.deepEqual(ids(settingsPieces('general', view(owner))), ['early', 'late', 'core']);
  assert.deepEqual(ids(pageTabs('team', view(owner))), ['own']);
  assert.deepEqual(ids(settingsTabs(view(part.id))), ['general']);
  assert.deepEqual(ids(settingsPieces('general', view(part.id))), ['core']);
  assert.deepEqual(pageTabs('team', view()), []);
  assert.deepEqual(ids(settingsPieces('general', undefined)), ['core']);
});
