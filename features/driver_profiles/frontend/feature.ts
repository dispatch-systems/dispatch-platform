import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Driver Profiles' frontend manifest, loaded up front. It has no page of its own: Driver
// Match's tab shows its roster.

export const feature: FrontendFeature = {
  name: 'driver_profiles',
  platformSlots: () => import('./platform-slots.js'),
  // A schedule meets it too, so the manifest words it.
  errors: {
    driver_profiles_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting the roster.",
  },
};
