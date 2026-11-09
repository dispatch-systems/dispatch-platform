import { createElement, lazy } from 'react';
import { FileWarning } from 'lucide-react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { conductUrl } from '../api/client.js';

// Conduct's frontend manifest, loaded up front: it stays small and loads the rest lazily.

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    conduct: true;
  }
}

const load = () => import('./index.js');
const ConductPage = lazy(() => load().then((module) => ({ default: module.ConductPage })));
const CategoriesSection = lazy(() =>
  load().then((module) => ({ default: module.CategoriesSection })),
);

export const feature: FrontendFeature = {
  name: 'conduct',
  routes: [
    {
      id: 'conduct',
      scope: 'dsp',
      label: 'Conduct',
      icon: FileWarning,
      nav: true,
      feature: 'conduct',
      permission: ({ view }) => can(view, 'conduct.use'),
      preload: load,
      render: ({ session, view }) => createElement(ConductPage, { key: view.token, session, view }),
      prefetch: ({ view, warm }) => {
        if (can(view, 'conduct.use')) warm([conductUrl]);
      },
    },
  ],
  // Its categories, on its page of Settings beside how long a writeup counts.
  settingsSections: [
    {
      id: 'conduct.categories',
      load,
      render: () => createElement(CategoriesSection),
      prefetch: () => [conductUrl],
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the activity log.
  platformSlots: () => import('./platform-slots.js'),
};
