import { createElement, lazy } from 'react';
import { CalendarDays } from 'lucide-react';
import type { DspView } from '../../../shared/contracts/index.js';
import { begins, collectionData, path } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { Access, FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { isTimecardDataReady, prefetchTimecard } from './prefetch.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    paycom: true;
    'paycom-settings': true;
  }
}

let pageReady: ((view: DspView) => boolean) | undefined;
const load = (access?: Access) =>
  import('./index.js').then(async (module) => {
    pageReady = module.isTimecardPageReady;
    if (access?.view) await module.preloadTimecardPage(access.view);
    return module;
  });
const loadSettings = () => import('./settings/index.js');
const PaycomPage = lazy(() => load().then((module) => ({ default: module.PaycomPage })));
const PaycomSettingsPage = lazy(() =>
  loadSettings().then((module) => ({ default: module.PaycomSettingsPage })),
);

export const feature: FrontendFeature = {
  name: 'timecard',
  routes: [
    {
      id: 'paycom',
      scope: 'dsp',
      label: 'Timecard',
      icon: CalendarDays,
      nav: true,
      feature: 'timecard',
      // The link stays put while a view loads; the page itself waits for the view.
      permission: ({ view }) => !view || can(view, 'timecard.view'),
      preload: load,
      render: ({ view }) => createElement(PaycomPage, { view }),
      ready: (view) => Boolean(pageReady?.(view) && isTimecardDataReady(view)),
      prefetch: ({ view, immediate }) => prefetchTimecard(view, immediate),
    },
    {
      id: 'paycom-settings',
      scope: 'dsp',
      label: 'Timecard',
      parent: 'paycom',
      nav: false,
      remembered: false,
      feature: 'timecard',
      permission: ({ view }) => can(view, 'timecard.manage'),
      preload: loadSettings,
      render: ({ view }) => createElement(PaycomSettingsPage, { dspId: view.dsp.id }),
      prefetch: ({ view, warm }) => {
        if (can(view, 'timecard.manage')) warm(['/api/dsp/schedules']);
      },
    },
  ],
  platformSlots: () => import('./platform-slots.js').then((module) => module.slots),
  errors: {
    meal_sync_paycom_required: 'Connect Paycom before syncing meal breaks.',
    meal_sync_flex_required: 'Connect Cortex in Settings → Connections before syncing Flex.',
    meal_sync_scope_required: 'Complete your DSP profile with a station code to sync Flex.',
    settings_changed_reload_before_saving:
      'These settings changed in another session. Discard your draft and try again.',
    connect_paycom_before_automatic_sync: 'Connect Paycom before turning on automatic sync.',
    employee_already_linked:
      'A Paycom employee can only link to one Flex driver. Review duplicate selections.',
    employee_link_source_missing:
      'This employee is no longer available. Refresh and review the links again.',
  },
  scheduleIssues: {
    schedule_scope_required: 'Run an initial Meal Break collection to set up the DSP’s station.',
  },
  cache: {
    collected: [
      '/api/dsp/timecards',
      '/api/dsp/employees',
      '/api/dsp/paycom/meal-breaks',
      '/api/dsp/paycom/settings',
      '/api/dsp/paycom/status',
    ],
    collection: (url, changes) => {
      const route = path(url);
      if (begins(url, '/api/dsp/paycom/status')) return true;
      return changes.some((change) => {
        if (change.provider === 'all') return true;
        if (change.provider === 'cortex' && route !== '/api/dsp/paycom/meal-breaks') return false;
        if (route === '/api/dsp/paycom/settings') return Boolean(change.roster);
        if (route === '/api/dsp/employees') return Boolean(change.roster);
        if (route.startsWith('/api/dsp/employees/'))
          return (
            !change.employeeCode ||
            decodeURIComponent(route.slice('/api/dsp/employees/'.length)) === change.employeeCode
          );
        const day = new URLSearchParams(url.split('?')[1]).get('date');
        return !day || !change.dates?.length || change.dates.includes(day);
      });
    },
    jobs: ['/api/dsp/paycom/status'],
    write: (write, url) => {
      if (write === '/api/dsp/paycom/settings') return collectionData(url) || path(url) === write;
      // A Driver Match decision moves drivers between rows of the meal-break comparison.
      if (write.startsWith('/api/dsp/driver-match'))
        return begins(url, '/api/dsp/paycom/meal-breaks');
      return undefined;
    },
  },
  readToggles: {
    label: 'Timecard',
    missing: 'timecard data',
    order: 20,
    sources: { timecards: 'Timecard', meal_breaks: 'Meal Breaks' },
    toggles: [
      { id: 'timecards', label: 'Timecards', missing: 'timecards', source: 'timecards' },
      { id: 'meal_breaks', label: 'Meal breaks', missing: 'meal breaks', source: 'meal_breaks' },
    ],
  },
};
