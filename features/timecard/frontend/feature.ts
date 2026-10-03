import { createElement, lazy } from 'react';
import { CalendarDays } from 'lucide-react';
import type { DspView } from '../../../shared/contracts/index.js';
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
      feature: 'timecard',
      permission: ({ view }) => can(view, 'timecard.manage'),
      preload: loadSettings,
      render: ({ view }) => createElement(PaycomSettingsPage, { dspId: view.dsp.id }),
      prefetch: ({ view, warm }) => {
        if (can(view, 'timecard.manage')) warm(['/api/dsp/schedules']);
      },
    },
  ],
};
