import assert from 'node:assert/strict';
import test from 'node:test';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { SettingsAnswer, SettingsSection } from '../../../../core/settings/api/index.js';
import { installFeatures } from '../../../../core/shell/frontend/runtime/slots.js';
import { drawnPages, openPage } from '../../frontend/tabs.js';

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
