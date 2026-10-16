import { createElement, lazy } from 'react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Driver Profiles' frontend manifest, loaded up front. It has no page of its own: Driver
// Match's Settings tab is its one screen.

const load = () => import('./index.js');
const loadBadge = () => import('./badge.js');
const DriverMatchSettings = lazy(() =>
  load().then((module) => ({ default: module.DriverMatchSettings })),
);

export const feature: FrontendFeature = {
  name: 'driver_profiles',
  settingsTabs: [
    {
      id: 'driver-match',
      label: 'Driver Match',
      part: 'driver_match',
      badge: { load: () => loadBadge().then((module) => module.DriverMatchTabLabel) },
      order: 40,
      visible: (view) => can(view, 'driver_match.manage'),
      load,
      render: ({ view }) =>
        view &&
        createElement(DriverMatchSettings, {
          timezone: view.dsp.timezone,
          canCollect: can(view, 'driver_profiles.collect'),
        }),
      // The badge counts the questions waiting on every tab; the open tab reads the whole roster.
      prefetch: (tab, view) =>
        can(view, 'driver_match.manage')
          ? [tab === 'driver-match' ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts']
          : [],
    },
  ],
  platformSlots: () => import('./platform-slots.js'),
  replies: () => import('../api/runtime.js'),
  // A schedule meets it too, so the manifest words it.
  errors: {
    driver_profiles_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting the roster.",
  },
  // A finished collection can bring new drivers to match, and the roster's own status.
  cache: {
    collected: ['/api/dsp/driver-match', '/api/dsp/driver-profiles/status'],
    write: (write, url) =>
      write.startsWith('/api/dsp/driver-match')
        ? begins(url, '/api/dsp/driver-match')
        : write.startsWith('/api/dsp/driver-profiles/')
          ? begins(url, '/api/dsp/driver-profiles/status')
          : undefined,
  },
};
