import { createElement, lazy } from 'react';
import { Fingerprint } from 'lucide-react';
import { peopleFeatures } from '../../../core/manifest/api/generated/people-features.js';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    'driver-match': true;
  }
}

// Driver Match's frontend manifest, loaded up front. Its one screen is a page of its own.

const load = () => import('./index.js');
const DriverMatchPage = lazy(() => load().then((module) => ({ default: module.DriverMatchPage })));
/** Whether the DSP has a feature on whose data names people: without one, there is no one. */
const named = (view?: DspView) => peopleFeatures.some((feature) => hasFeature(view, feature));
const manages = (view?: DspView) => can(view, 'driver_match.manage') && named(view);

export const feature: FrontendFeature = {
  name: 'driver_match',
  routes: [
    {
      id: 'driver-match',
      scope: 'dsp',
      label: 'Driver Match',
      icon: Fingerprint,
      // How many questions wait.
      badge: { load: () => import('./badge.js').then((module) => module.DriverMatchCount) },
      nav: true,
      feature: 'driver_match',
      permission: ({ view }) => manages(view),
      // It was a tab of Settings.
      formerly: ['settings?tab=driver-match'],
      preload: load,
      render: ({ view }) => createElement(DriverMatchPage, { timezone: view.dsp.timezone }),
      prefetch: ({ view, warm }) => {
        if (manages(view)) warm(['/api/dsp/driver-match']);
      },
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
