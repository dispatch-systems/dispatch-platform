import { createElement, lazy } from 'react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { replies } from '../../../shared/contracts/runtime-driver-match.js';

const load = () => import('./index.js');
const loadBadge = () => import('./badge.js');
const DriverMatchSettings = lazy(() =>
  load().then((module) => ({ default: module.DriverMatchSettings })),
);

export const feature: FrontendFeature = {
  name: 'driver_match',
  settingsTabs: [
    {
      id: 'driver-match',
      label: 'Driver Match',
      badge: { load: () => loadBadge().then((module) => module.DriverMatchTabLabel) },
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
  platformSlots: () => import('./platform-slots.js'),
  replies,
  // A finished collection can bring new drivers to match.
  cache: {
    collected: ['/api/dsp/driver-match'],
    write: (write, url) =>
      write.startsWith('/api/dsp/driver-match') ? begins(url, '/api/dsp/driver-match') : undefined,
  },
};
