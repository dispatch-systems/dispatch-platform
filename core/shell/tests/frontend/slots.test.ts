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
  connectionIntegrations,
  connectionPieces,
  connectionSections,
  errorLabelOf,
  installFeatures,
  isLongPoll,
  pageTabs,
  runWording,
  scheduleIssueOf,
  settingsRead,
  auditWording,
  loadPlatformSlots,
  movedTo,
  switchIcon,
  type ConnectionCard,
  type ConnectionIntegration,
  type ConnectionPiece,
  type ConnectionSection,
  type DspRoute,
  type DspRouteId,
  type PageTab,
  type PlatformSlots,
  type RunWording,
} from '../../frontend/runtime/slots.js';
import { dataCache } from '../../frontend/runtime/data-cache.js';
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
    { name: 'epsilon', cache: { jobs: ['/api/dsp/epsilon/jobs'] } },
  ]);
  assert.equal(cacheRules().length, 3);
  const change = { provider: 'fixture', dates: [], employeeCode: null, roster: false };
  assert.equal(collectionData('/api/dsp/alpha/days?date=2026-09-22'), true);
  assert.equal(collectionData('/api/dsp/delta'), false);
  assert.equal(collectionAffects('/api/dsp/alpha/days', [change]), false);
  assert.equal(collectionAffects('/api/dsp/alpha/days', [{ ...change, roster: true }]), true);
  assert.equal(collectionAffects('/api/dsp/alpha/status', []), true);
  assert.equal(collectionAffects('/api/dsp/gamma/rows', [change]), true);
  // Core keeps no jobs of its own to read.
  assert.equal(collectionAffects('/api/dsp/jobs', []), false);
  assert.equal(mutationAffects('/api/dsp/alpha/edit', '/api/dsp/alpha/days'), true);
  assert.equal(mutationAffects('/api/dsp/alpha/edit', '/api/dsp/gamma/rows'), true);
  assert.equal(mutationAffects('/api/dsp/alpha/edit', '/api/dsp/gamma/other'), false);
  // A write an owner knows changes nothing else, the jobs included.
  assert.equal(mutationAffects('/api/dsp/alpha/collect', '/api/dsp/epsilon/jobs'), false);
  // A write under an owner's job read starts or cancels a job, which every job read follows.
  assert.equal(mutationAffects('/api/dsp/epsilon/jobs/1/cancel', '/api/dsp/alpha/status'), true);
  assert.equal(mutationAffects('/api/dsp/epsilon/jobs', '/api/dsp/gamma/rows'), false);
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

test('a link to Settings opens the page that draws the settings, on the page it names, unless the person is known not to have it', () => {
  installFeatures([
    { name: 'alpha', routes: [page('board'), { ...page('options'), hostsSettings: true }] },
  ]);
  assert.equal(settingsHash('dsp_settings'), '#dsp/dsp_settings/options');
  // Which pages a person has is the server's to say: until Settings is read, a link names it.
  const view = { dsp: { id: 'dsp_settings' } } as DspView;
  assert.equal(settingsHash('dsp_settings', 'data', view), '#dsp/dsp_settings/options?tab=data');
  // What the page opens on goes beside it.
  assert.equal(
    settingsHash('dsp_settings', 'data', view, { workspace: 'T1' }),
    '#dsp/dsp_settings/options?tab=data&workspace=T1',
  );
  dataCache.put(settingsRead, {
    pages: [{ id: 'data', label: 'Data', group: 'dsp', order: 50 }],
    sections: [],
  });
  assert.equal(settingsHash('dsp_settings', 'data', view), '#dsp/dsp_settings/options?tab=data');
  assert.equal(settingsHash('dsp_settings', 'gone', view), undefined);
  // What one DSP's Settings holds says nothing of another's.
  assert.equal(settingsHash('dsp_other', 'gone', view), '#dsp/dsp_other/options?tab=gone');
  dataCache.clear();
  installFeatures([{ name: 'beta', routes: [page('board')] }]);
  assert.equal(settingsHash('dsp_settings'), undefined);
  assert.equal(settingsHash('dsp_settings', 'data'), undefined);
});

test('a link to where a page once was opens it where it is now, and a page of Settings with the rest of the link', () => {
  installFeatures([
    { name: 'alpha', routes: [{ ...page('options'), hostsSettings: true }] },
    { name: 'beta', routes: [{ ...page('match'), formerly: ['options?tab=match'] }] },
    { name: 'gamma', movedToSettings: { gamma: 'gamma' } },
  ]);
  const moved = (page: string, query = '') => movedTo(page, new URLSearchParams(query));
  assert.deepEqual(moved('options', 'tab=match'), { page: 'match' });
  assert.equal(moved('options', 'tab=data'), undefined);
  assert.deepEqual(moved('gamma', 'workspace=T1&section=channels'), {
    page: 'options',
    query: { tab: 'gamma', workspace: 'T1', section: 'channels' },
  });
  // Without a page that draws Settings, a page of it is nowhere.
  installFeatures([{ name: 'gamma', movedToSettings: { gamma: 'gamma' } }]);
  assert.equal(moved('gamma'), undefined);
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
  const card = (
    id: string,
    section: ConnectionPiece['section'],
    order: number,
    extra: Partial<ConnectionPiece> = {},
  ): ConnectionPiece => ({
    id,
    section,
    order,
    reads: [],
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
  const section = (
    id: string,
    order: number,
    extra: Partial<ConnectionSection> = {},
  ): ConnectionSection => ({
    connection: 'chat',
    id,
    label: id,
    order,
    render: () => id,
    ...extra,
  });
  const integration = (
    id: string,
    order: number,
    extra: Partial<ConnectionIntegration> = {},
  ): ConnectionIntegration => ({
    connection: 'chat',
    id,
    label: id,
    icon: Award,
    description: id,
    steps: [],
    sees: [],
    turningOn: [],
    turningOff: [],
    order,
    ...extra,
  });
  installFeatures([
    {
      name: 'core-owner',
      connectionSections: [section('core', 50)],
    },
    {
      name: owner,
      pageTabs: [pageTab('own'), pageTab('of-part', { part: part.id })],
      connectionPieces: [
        card('account-late', 'dsp', 20),
        card('account', 'dsp', 10),
        card('mine', 'personal', 10),
        card('unseen-mine', 'personal', 1, { visible: () => false }),
      ],
      connectionSections: [
        section('late', 30),
        section('early', 10),
        section('of-part', 20, { part: part.id }),
        section('elsewhere', 1, { connection: 'mail' }),
        section('unseen', 1, { visible: () => false }),
      ],
      connectionIntegrations: [
        integration('late', 30),
        integration('early', 10),
        integration('of-part', 20, { part: part.id }),
        integration('elsewhere', 1, { connection: 'mail' }),
        integration('unseen', 1, { visible: () => false }),
      ],
    },
  ]);
  const ids = (each: readonly { id: string }[]) => each.map(({ id }) => id);
  assert.deepEqual(ids(pageTabs('team', view(owner, part.id))), ['own', 'of-part']);
  assert.deepEqual(ids(connectionSections('chat', view(owner, part.id))), [
    'early',
    'of-part',
    'late',
    'core',
  ]);
  const integrations = connectionIntegrations('chat', view(owner, part.id));
  assert.deepEqual(ids(integrations), ['early', 'of-part', 'late']);
  // Each says which feature it's part of, by the name the feature goes by.
  const named = featureCatalog.find((f) => f.id === owner)?.label;
  assert.ok(named && integrations.every(({ from }) => from === named));
  assert.deepEqual(ids(connectionPieces('dsp', view(owner))), ['account', 'account-late']);
  assert.deepEqual(ids(connectionPieces('personal', view(owner))), ['mine']);
  // Every card loads with the Connections tab, whoever sees it.
  assert.deepEqual(ids(connectionPieces('personal')), ['unseen-mine', 'mine']);
  // Without the part, its own go; without the feature, all of the feature's.
  assert.deepEqual(ids(pageTabs('team', view(owner))), ['own']);
  assert.deepEqual(ids(connectionSections('chat', view(owner))), ['early', 'late', 'core']);
  assert.deepEqual(ids(connectionIntegrations('chat', view(owner))), ['early', 'late']);
  assert.deepEqual(pageTabs('team', view()), []);
  assert.deepEqual(connectionPieces('dsp', view(part.id)), []);
  assert.deepEqual(ids(connectionSections('chat', view(part.id))), ['core']);
  assert.deepEqual(connectionIntegrations('chat', view(part.id)), []);
});
