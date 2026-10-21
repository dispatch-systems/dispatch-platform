import { Contact } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Driver Profiles' frontend manifest, loaded up front. It has no page of its own: the roster is
// collected, on demand or by its rules, from Settings' Collectors page, which shows its icon.

export const feature: FrontendFeature = {
  name: 'driver_profiles',
  icon: Contact,
  platformSlots: () => import('./platform-slots.js'),
  // Collecting the roster meets it, so the manifest words it.
  errors: {
    driver_profiles_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting the roster.",
  },
};
