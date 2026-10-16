import { createElement, lazy } from 'react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Driver Profiles' frontend manifest, loaded up front. It has no page of its own yet: its
// roster bar sits at the top of Driver Match's tab.

const load = () => import('./index.js');
const RosterBar = lazy(() => load().then((module) => ({ default: module.RosterBar })));

export const feature: FrontendFeature = {
  name: 'driver_profiles',
  settingsPieces: [
    {
      tab: 'driver-match',
      id: 'roster',
      order: 10,
      above: true,
      visible: (view) => can(view, 'driver_profiles.collect'),
      load,
      render: ({ view }) => view && createElement(RosterBar, { timezone: view.dsp.timezone }),
    },
  ],
  platformSlots: () => import('./platform-slots.js'),
  // A schedule meets it too, so the manifest words it.
  errors: {
    driver_profiles_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting the roster.",
  },
  // A finished collection changes the roster's own status.
  cache: {
    collected: ['/api/dsp/driver-profiles/status'],
    write: (write, url) =>
      write.startsWith('/api/dsp/driver-profiles/')
        ? begins(url, '/api/dsp/driver-profiles/status')
        : undefined,
  },
};
