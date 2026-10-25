import { createElement, lazy } from 'react';
import { CalendarDays } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { begins, path } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import {
  settingsRead,
  type Access,
  type FrontendFeature,
} from '../../../core/shell/frontend/runtime/slots.js';
import {
  employeesPath,
  mealBreaksPath,
  mealPeriodGapsPath,
  mealPeriodPath,
  paycomStatusPath,
} from '../api/urls.js';
import { isTimecardDataReady, prefetchTimecard } from './prefetch.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    paycom: true;
  }
}

let pageReady: ((view: DspView) => boolean) | undefined;
const load = (access?: Access) =>
  import('./index.js').then(async (module) => {
    pageReady = module.isTimecardPageReady;
    if (access?.view) await module.preloadTimecardPage(access.view);
    return module;
  });
const PaycomPage = lazy(() => load().then((module) => ({ default: module.PaycomPage })));

export const feature: FrontendFeature = {
  name: 'timecard',
  routes: [
    {
      id: 'paycom',
      scope: 'dsp',
      label: 'Timecard',
      icon: CalendarDays,
      nav: true,
      tabs: true,
      feature: 'timecard',
      // The link stays put while a view loads; the page itself waits for the view.
      permission: ({ view }) => !view || can(view, 'timecard.view'),
      preload: load,
      render: ({ view }) => createElement(PaycomPage, { view }),
      ready: (view) => Boolean(pageReady?.(view) && isTimecardDataReady(view)),
      prefetch: ({ view, immediate }) => prefetchTimecard(view, immediate),
    },
  ],
  movedToSettings: { 'paycom-settings': 'timecard' },
  platformSlots: () => import('./platform-slots.js'),
  collected: () => import('./collected.js'),
  replies: () => import('../api/runtime.js'),
  scheduleIssues: {
    schedule_scope_required: 'Run an initial Meal Break collection to set up the DSP’s station.',
  },
  cache: {
    collected: [
      '/api/dsp/jobs',
      '/api/dsp/timecards',
      employeesPath,
      mealBreaksPath,
      paycomStatusPath,
      settingsRead,
    ],
    collection: (url, changes) => {
      const route = path(url);
      if (begins(url, paycomStatusPath, '/api/dsp/jobs')) return true;
      return changes.some((change) => {
        if (change.provider === 'all') return true;
        if (change.provider === 'cortex' && !route.startsWith(mealBreaksPath)) return false;
        // A day's publication or match can change any summary whose pay period contains it.
        if (route === mealPeriodPath || route === mealPeriodGapsPath) return true;
        // Its settings list the roster's departments and stations.
        if (route === settingsRead) return Boolean(change.roster);
        if (route === employeesPath) return Boolean(change.roster);
        if (route.startsWith(`${employeesPath}/`))
          return (
            !change.employeeCode ||
            decodeURIComponent(route.slice(employeesPath.length + 1)) === change.employeeCode
          );
        const day = new URLSearchParams(url.split('?')[1]).get('date');
        return !day || !change.dates?.length || change.dates.includes(day);
      });
    },
    jobs: ['/api/dsp/jobs', paycomStatusPath],
    write: (write, url) => {
      // A Driver Match decision moves drivers between rows of the meal-break comparison, and
      // changes whose photo an employee shows.
      if (write.startsWith('/api/dsp/driver-match'))
        return begins(url, mealBreaksPath) || begins(url, employeesPath);
      return undefined;
    },
  },
};
