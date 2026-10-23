import assert from 'node:assert/strict';
import test from 'node:test';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { SettingsAnswer, SettingsSection } from '../../../../core/settings/api/index.js';
import {
  installFeatures,
  type DspRoute,
  type DspRouteId,
} from '../../../../core/shell/frontend/runtime/slots.js';
import { drawnPages, listedPages, openPage, sectionsOn } from '../../frontend/pages.js';

// Settings draws the pages the server says the member has, each with a section on it to draw:
// a custom section only with the component its owner registers, while that has something to
// show them.
test('Settings lists each page with a section it draws, and opens on the one named or else the first', () => {
  const drawn = { load: () => Promise.resolve(), render: () => null };
  installFeatures([
    {
      name: 'owner',
      settingsSections: [
        { id: 'owner.shown', ...drawn },
        { id: 'owner.nothing', visible: () => false, ...drawn },
      ],
    },
  ]);
  const section = (id: string, page: string, custom: boolean): SettingsSection => ({
    id,
    page,
    label: id,
    order: 10,
    scope: 'dsp',
    custom,
    collection: null,
    fields: [],
    values: {},
    revision: 0,
    sources: [],
    keywords: [],
  });
  const settings: SettingsAnswer = {
    pages: ['general', 'connections', 'data', 'unregistered'].map((id, order) => ({
      id,
      label: id,
      group: 'dsp',
      order,
    })),
    sections: [
      section('owner.shown', 'general', true),
      section('owner.nothing', 'connections', true),
      section('owner.declared', 'data', false),
      section('elsewhere.custom', 'unregistered', true),
    ],
  };
  const view = {} as DspView;
  assert.deepEqual(
    drawnPages(settings, view).map(({ id }) => id),
    ['general', 'data'],
  );
  assert.equal(openPage(settings, view, 'data'), 'data');
  assert.equal(openPage(settings, view, 'connections'), 'general');
  assert.equal(openPage(settings, view, null), 'general');
});

// A section that goes with a collection is drawn with it by the Collectors page, not on its own.
test('a section that goes with a collection is left to the Collectors page to draw', () => {
  const drawn = { load: () => Promise.resolve(), render: () => null };
  installFeatures([
    { name: 'collection', settingsSections: [{ id: 'collection.collectors', ...drawn }] },
  ]);
  const section = (id: string, custom: boolean, collection: string | null): SettingsSection => ({
    id,
    page: 'collectors',
    label: id,
    order: 0,
    scope: 'dsp',
    custom,
    collection,
    fields: [],
    values: {},
    revision: 0,
    sources: [],
    keywords: [],
  });
  const settings: SettingsAnswer = {
    pages: [{ id: 'collectors', label: 'Collectors', group: 'dsp', order: 40 }],
    sections: [
      section('collection.collectors', true, null),
      section('daily_performance.collection', false, 'daily_performance'),
    ],
  };
  const view = {} as DspView;
  assert.deepEqual(
    sectionsOn(settings, view, 'collectors').map(({ id }) => id),
    ['collection.collectors'],
  );
});

// A feature puts a page of its own in Settings by naming Settings' sidebar its parent: the
// sidebar lists it under Features, in its feature's place among the features' pages.
test("Settings' sidebar lists a page a feature puts under it among Features, in its feature's place", () => {
  const page = (id: string, more: Partial<DspRoute> = {}): DspRoute => ({
    id: id as DspRouteId,
    scope: 'dsp',
    label: id,
    parent: 'settings',
    nav: true,
    preload: () => Promise.resolve(),
    render: () => null,
    ...more,
  });
  installFeatures([
    { name: 'routes' },
    { name: 'driver_match', routes: [page('driver-match', { feature: 'driver_match' })] },
    { name: 'slack' },
    {
      name: 'settings',
      routes: [page('settings', { hostsSettings: true })],
      sidebars: [{ id: 'settings', scope: 'dsp', label: 'Settings' }],
    },
  ]);
  const declared = (id: string, on: string): SettingsSection => ({
    id,
    page: on,
    label: id,
    order: 10,
    scope: 'dsp',
    custom: false,
    collection: null,
    fields: [],
    values: {},
    revision: 0,
    sources: [],
    keywords: [],
  });
  const settings: SettingsAnswer = {
    pages: [
      { id: 'general', label: 'Profile', group: 'personal', order: 10 },
      { id: 'routes', label: 'Routes', group: 'features', order: 40 },
      { id: 'slack', label: 'Slack', group: 'features', order: 90 },
    ],
    sections: [
      declared('accounts.profile', 'general'),
      declared('routes.retention', 'routes'),
      declared('slack.workspaces', 'slack'),
    ],
  };
  const view = { features: ['driver_match'] } as unknown as DspView;
  const listed = listedPages(settings, { session: {} as never, view });
  assert.deepEqual(
    listed.map(({ id, group }) => `${group}: ${id}`),
    ['personal: general', 'features: routes', 'features: driver-match', 'features: slack'],
  );
  assert.equal(listed[2]?.route?.id, 'driver-match');
});
