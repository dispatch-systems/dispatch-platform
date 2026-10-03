import { createElement, lazy } from 'react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

const load = () => import('./settings/RouteDataSettings.js');
const RouteDataSettings = lazy(() =>
  load().then((module) => ({ default: module.RouteDataSettings })),
);

export const feature: FrontendFeature = {
  name: 'routes',
  settingsTabs: [
    {
      id: 'data',
      label: 'Data',
      order: 50,
      visible: (view) => can(view, 'routes.manage'),
      load,
      render: ({ view }) =>
        view && createElement(RouteDataSettings, { timeZone: view.dsp.timezone }),
      prefetch: (tab, view) =>
        tab === 'data' && can(view, 'routes.manage') ? ['/api/dsp/routes/retention'] : [],
    },
  ],
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
};
