import { Truck } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Vans' frontend manifest, loaded up front. It has no page of its own: the fleet is collected, on
// demand or by its rules, from Settings' Collectors page, which shows its icon.

export const feature: FrontendFeature = {
  name: 'vans',
  icon: Truck,
  platformSlots: () => import('./platform-slots.js'),
  collected: () => import('./collected.js'),
  // Collecting the fleet meets it, so the manifest words it.
  errors: {
    vans_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting vans.",
  },
};
