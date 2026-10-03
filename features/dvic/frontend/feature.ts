import { createElement, lazy } from 'react';
import { ClipboardCheck } from 'lucide-react';
import type { DspView } from '../../../shared/contracts/index.js';
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
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
  switch: { id: 'dvic', icon: ClipboardCheck },
  readToggles: {
    label: 'DVIC',
    missing: 'DVIC inspections',
    order: 40,
    sources: { dvic: 'DVIC' },
    toggles: [
      { id: 'dvic', label: 'DVIC inspections', missing: 'DVIC inspections', source: 'dvic' },
    ],
  },
};
