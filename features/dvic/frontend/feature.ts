import { createElement, lazy } from 'react';
import { ClipboardCheck } from 'lucide-react';
import type { DspView } from '../../../shared/contracts/index.js';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

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
  platformSlots: () => import('./platform-slots.js').then((module) => module.slots),
  errors: {
    dvic_station_required:
      'Set your station code in the DSP profile before collecting DVIC reports.',
    dvic_week_not_available: 'That report week is not available yet.',
  },
  cache: {
    connections: ['/api/dsp/dvic/'],
    write: (write, url) =>
      write.startsWith('/api/dsp/dvic/') ? begins(url, '/api/dsp/dvic/') : undefined,
  },
};
