import { createElement, lazy } from 'react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

const load = () => import('./index.js');
const loadBadge = () => import('./badge.js');
const DriverMatchSettings = lazy(() =>
  load().then((module) => ({ default: module.DriverMatchSettings })),
);
const DriverMatchTabLabel = lazy(() =>
  loadBadge().then((module) => ({ default: module.DriverMatchTabLabel })),
);

export const feature: FrontendFeature = {
  name: 'driver_match',
  settingsTabs: [
    {
      id: 'driver-match',
      label: 'Driver Match',
      badge: { load: loadBadge, Label: DriverMatchTabLabel },
      order: 40,
      visible: (view) => can(view, 'driver_match.manage'),
      load,
      render: ({ view }) =>
        view && createElement(DriverMatchSettings, { timezone: view.dsp.timezone }),
      // The badge counts the pairs to review on every tab; the open tab reads the whole roster.
      prefetch: (tab, view) =>
        can(view, 'driver_match.manage')
          ? [tab === 'driver-match' ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts']
          : [],
    },
  ],
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
};
