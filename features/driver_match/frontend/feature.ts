import { createElement, lazy } from 'react';
import { peopleFeatures } from '../../../core/manifest/api/generated/people-features.js';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Driver Match's frontend manifest, loaded up front. Its one screen is a tab of Settings.

const load = () => import('./index.js');
const loadBadge = () => import('./badge.js');
const DriverMatchSettings = lazy(() =>
  load().then((module) => ({ default: module.DriverMatchSettings })),
);
/** Whether the DSP has a feature on whose data names people: without one, there is no one. */
const named = (view?: DspView) => peopleFeatures.some((feature) => hasFeature(view, feature));

export const feature: FrontendFeature = {
  name: 'driver_match',
  settingsTabs: [
    {
      id: 'driver-match',
      label: 'Driver Match',
      badge: { load: () => loadBadge().then((module) => module.DriverMatchTabLabel) },
      order: 40,
      visible: (view) => can(view, 'driver_match.manage') && named(view),
      load,
      render: ({ view }) =>
        view && createElement(DriverMatchSettings, { timezone: view.dsp.timezone }),
      // The badge counts the questions waiting on every tab; the open tab reads the whole roster.
      prefetch: (tab, view) =>
        can(view, 'driver_match.manage') && named(view)
          ? [tab === 'driver-match' ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts']
          : [],
    },
  ],
  platformSlots: () => import('./platform-slots.js'),
  replies: () => import('../api/runtime.js'),
  // A finished collection can bring new people to match.
  cache: {
    collected: ['/api/dsp/driver-match'],
    write: (write, url) =>
      write.startsWith('/api/dsp/driver-match') ? begins(url, '/api/dsp/driver-match') : undefined,
  },
};
