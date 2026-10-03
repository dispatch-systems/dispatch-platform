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
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
  switch: { id: 'routes', icon: () => import('./switch-icon.js').then((module) => module.icon) },
  errors: {
    invalid_retention: 'Choose a retention window from 30 to 3,650 days.',
    routes_day_outside_retention:
      'That day is older than your route data retention window. Lengthen the window first.',
  },
  readToggles: {
    label: 'Routes',
    missing: 'route data',
    order: 10,
    sources: { routes: 'Routes' },
    toggles: [
      { id: 'routes', label: 'Routes & packages', missing: 'routes', source: 'routes' },
      {
        id: 'locations',
        label: 'Delivery addresses & GPS',
        hint: 'Stop addresses and GPS points',
        missing: 'delivery addresses',
        source: 'routes',
        with: 'routes',
        optIn: true,
      },
    ],
  },
};
