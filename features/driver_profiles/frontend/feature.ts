import { Contact } from 'lucide-react';
import { createElement, lazy } from 'react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Driver Profiles' frontend manifest, loaded up front. It has no page of its own: its roster
// bar is its custom section of Settings, on its Settings page beside its schedules.

const load = () => import('./index.js');
const RosterBar = lazy(() => load().then((module) => ({ default: module.RosterBar })));

export const feature: FrontendFeature = {
  name: 'driver_profiles',
  icon: Contact,
  settingsSections: [
    {
      id: 'driver_profiles.roster',
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
