import assert from 'node:assert/strict';
import test from 'node:test';
import type {
  SettingsField,
  SettingsPage,
  SettingsSection,
} from '../../../../core/settings/api/index.js';
import { search, type Marked } from '../../frontend/search/search.js';

// Settings' search finds the member's pages by what they hold, a page once, ranked by the best
// of what matched there: its name, then a section's, then a field's, an option's or a word the
// section is also known by, then a collector its data comes from, then a value held as text.
// Every word typed begins a word there, in any case.

const page = (id: string, label: string, order: number): SettingsPage => ({
  id,
  label,
  group: 'dsp',
  order,
});
const section = (
  id: string,
  on: string,
  label: string,
  more: Partial<SettingsSection> = {},
): SettingsSection => ({
  id,
  page: on,
  label,
  order: 10,
  scope: 'dsp',
  custom: false,
  fields: [],
  values: {},
  revision: 0,
  sources: [],
  keywords: [],
  ...more,
});
const field = (id: string, label: string, kind: Partial<SettingsField>) =>
  ({ id, label, ...kind }) as SettingsField;

const pages = [
  page('security', 'Security', 1),
  page('dsp', 'DSP profile', 2),
  page('connections', 'Connections', 3),
  page('data', 'Data', 4),
  page('timecard', 'Timecard', 5),
  page('dvic', 'DVIC', 6),
];
const sections = [
  section('accounts.security', 'security', 'Security', {
    custom: true,
    keywords: ['password', 'passkey'],
  }),
  section('settings.profile', 'dsp', 'DSP profile', {
    fields: [
      field('name', 'Name', { kind: 'text', min: 1, max: 80, code: false, locked: false }),
      field('timezone', 'Time zone', { kind: 'timezone' }),
    ],
    values: { name: 'Northline Logistics', timezone: 'America/Chicago' },
  }),
  section('collection.connections', 'connections', 'Connections', {
    custom: true,
    keywords: ['Cortex', 'Paycom'],
  }),
  section('routes.retention', 'data', 'Route data', {
    sources: ['Cortex'],
    fields: [
      field('days', 'Keep for', {
        kind: 'number',
        min: 30,
        max: 3650,
        unit: 'days',
        presets: [{ value: 30, label: '30 days' }],
        none: 'Every day',
      }),
    ],
  }),
  section('timecard.meal_breaks', 'timecard', 'Meal breaks', {
    sources: ['Paycom', 'Cortex'],
    fields: [field('runs', 'Collection schedules', { kind: 'schedules' })],
  }),
  section('dvic.inspections', 'dvic', 'Inspections', { sources: ['Cortex'] }),
];

/** Marked text as it reads, with what matched in brackets. */
const read = (text: Marked | undefined) =>
  text?.map((part) => (part.mark ? `[${part.text}]` : part.text)).join('');
const found = (query: string) =>
  search(pages, sections, query).map(
    (each) =>
      `${each.page.id} ${read(each.name)} | ${read(each.what) ?? ''} | ${each.section ?? ''}`,
  );

test('Settings search finds each page once, by the best of what it holds that every word typed begins', () => {
  // A word the section is also known by ranks above the collectors data comes from; one page
  // with two of them is found once.
  assert.deepEqual(found('cortex'), [
    'connections Connections | [Cortex] | collection.connections',
    'data Data | Route data from [Cortex] | routes.retention',
    'timecard Timecard | Meal breaks from [Cortex] | timecard.meal_breaks',
    'dvic DVIC | Inspections from [Cortex] | dvic.inspections',
  ]);
  // A page's name ranks above a field of a page listed before it; whose name matched lists its
  // sections, but one named as it is.
  assert.deepEqual(found('TIME'), [
    'timecard [Time]card | Meal breaks | ',
    'dsp DSP profile | [Time] zone | settings.profile',
  ]);
  assert.deepEqual(found('data'), ['data [Data] | Route data | ']);
  // The start of a word, in any case; not the middle of one.
  assert.deepEqual(found('sched'), [
    'timecard Timecard | Collection [sched]ules | timecard.meal_breaks',
  ]);
  assert.deepEqual(found('hedule'), []);
  assert.deepEqual(found('pass'), ['security Security | [Pass]word | accounts.security']);
  // Every word typed, in any order; an option's label and a value read after their field's.
  assert.deepEqual(found('day every'), ['data Data | Keep for: [Every] [day] | routes.retention']);
  assert.deepEqual(found('northline'), [
    'dsp DSP profile | Name: [Northline] Logistics | settings.profile',
  ]);
  assert.deepEqual(found('  '), []);
});
