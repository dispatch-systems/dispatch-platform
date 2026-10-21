import { Truck } from 'lucide-react';
import { createElement, lazy } from 'react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Vans' frontend manifest, loaded up front. It has no page of its own: its fleet bar is its
// custom section of Settings, on its Settings page; when the fleet is collected is set on
// Settings' Collectors page.

const load = () => import('./index.js');
const FleetBar = lazy(() => load().then((module) => ({ default: module.FleetBar })));

export const feature: FrontendFeature = {
  name: 'vans',
  // The icon its page of Settings shows.
  icon: Truck,
  settingsSections: [
    {
      id: 'vans.fleet',
      load,
      render: ({ view }) => view && createElement(FleetBar, { timezone: view.dsp.timezone }),
    },
  ],
  platformSlots: () => import('./platform-slots.js'),
  // A schedule meets it too, so the manifest words it.
  errors: {
    vans_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting vans.",
  },
  // A finished collection changes the fleet's own status.
  cache: {
    collected: ['/api/dsp/vans/status'],
    write: (write, url) =>
      write.startsWith('/api/dsp/vans/') ? begins(url, '/api/dsp/vans/status') : undefined,
  },
};
