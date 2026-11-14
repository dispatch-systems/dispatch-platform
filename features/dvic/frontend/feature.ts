import { createElement, lazy } from 'react';
import { ClipboardCheck } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { sizesUpTo, type FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    dvic: true;
  }
}

let pageReady: ((view: DspView) => boolean) | undefined;
const load = () =>
  import('./index.js').then((module) => {
    pageReady = module.isDvicPageReady;
    return module;
  });
const DvicPage = lazy(() => load().then((module) => ({ default: module.DvicPage })));

export const feature: FrontendFeature = {
  name: 'dvic',
  routes: [
    {
      id: 'dvic',
      scope: 'dsp',
      label: 'DVIC',
      icon: ClipboardCheck,
      nav: true,
      tabs: true,
      feature: 'dvic',
      permission: ({ view }) => can(view, 'dvic.view'),
      preload: load,
      render: ({ view }) => createElement(DvicPage, { key: view.token, view }),
      ready: (view) => Boolean(pageReady?.(view)),
      prefetch: ({ view, warm }) => {
        if (can(view, 'dvic.view')) warm(['/api/dsp/dvic/status']);
      },
    },
  ],
  // Who was short on the latest collected day, for each member's Home page.
  widgets: [
    {
      id: 'dvic.latest',
      label: 'Latest DVIC',
      icon: ClipboardCheck,
      sizes: sizesUpTo({ columns: 4, rows: 4 }, { columns: 2, rows: 2 }),
      permission: ({ view }) => can(view, 'dvic.view'),
      part: 'dvic.day',
      load: () => import('./widgets/LatestDvicWidget.js').then((module) => module.LatestDvicWidget),
    },
  ],
  platformSlots: () => import('./platform-slots.js'),
  collected: () => import('./collected.js'),
  errors: {
    dvic_station_required:
      'Set your station code in the DSP profile before collecting DVIC reports.',
  },
  cache: {
    // A finished Cortex collection rereads the status; the inspections follow what it published.
    collected: ['/api/dsp/dvic/status'],
    collection: (_, changes) =>
      changes.some((change) => change.provider === 'all' || change.provider === 'cortex'),
    connections: ['/api/dsp/dvic/'],
    write: (write, url) =>
      write.startsWith('/api/dsp/dvic/') ? begins(url, '/api/dsp/dvic/') : undefined,
  },
};
