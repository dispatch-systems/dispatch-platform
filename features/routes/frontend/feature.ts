import { createElement, lazy } from 'react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

const load = () => import('./settings/RoutesSettings.js');
const RoutesSettings = lazy(() => load().then((module) => ({ default: module.RoutesSettings })));

export const feature: FrontendFeature = {
  name: 'routes',
  settingsTabs: [
    {
      id: 'data',
      label: 'Data',
      order: 50,
      visible: (view) => can(view, 'routes.manage'),
      load,
      render: ({ view }) => view && createElement(RoutesSettings, { timeZone: view.dsp.timezone }),
      prefetch: (tab, view) =>
        tab === 'data' && can(view, 'routes.manage') ? ['/api/dsp/routes/retention'] : [],
    },
  ],
  platformSlots: () => import('./platform-slots.js').then((module) => module.slots),
  errors: {
    invalid_retention: 'Choose a retention window from 30 to 3,650 days.',
    routes_day_outside_retention:
      'That day is older than your route data retention window. Lengthen the window first.',
  },
};
