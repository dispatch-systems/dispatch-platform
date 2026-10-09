import { createElement, lazy } from 'react';
import { CalendarDays } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { begins, collectionData, path } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { Access, FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { replies } from '../api/runtime.js';
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
  platformSlots: () => import('./platform-slots.js'),
  replies,
  scheduleIssues: {
    schedule_scope_required: 'Run an initial Meal Break collection to set up the DSP’s station.',
  },
  cache: {
    collected: [
      '/api/dsp/jobs',
      '/api/dsp/timecards',
      '/api/dsp/employees',
      '/api/dsp/paycom/meal-breaks',
      '/api/dsp/paycom/settings',
      '/api/dsp/paycom/status',
    ],
    collection: (url, changes) => {
      const route = path(url);
      if (begins(url, '/api/dsp/paycom/status', '/api/dsp/jobs')) return true;
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
    jobs: ['/api/dsp/jobs', '/api/dsp/paycom/status'],
    connections: ['/api/dsp/schedules'],
    write: (write, url) => {
      if (write.startsWith('/api/dsp/schedules')) return begins(url, '/api/dsp/schedules');
      if (write === '/api/dsp/paycom/settings') return collectionData(url) || path(url) === write;
      // A Driver Match decision moves drivers between rows of the meal-break comparison.
      if (write.startsWith('/api/dsp/driver-match'))
        return begins(url, '/api/dsp/paycom/meal-breaks');
      return undefined;
    },
  },
};
