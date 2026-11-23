import { CalendarDays } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Schedules' frontend manifest, loaded up front. It has no page of its own: the week is
// collected, on demand or by its rules, from Settings' Collectors page, which shows its icon.

export const feature: FrontendFeature = {
  name: 'shifts',
  icon: CalendarDays,
  platformSlots: () => import('./platform-slots.js'),
  collected: () => import('./collected.js'),
  // Collecting the week meets them, so the manifest words them.
  errors: {
    shifts_station_required: "Set your DSP's station in its profile before collecting schedules.",
    shifts_abbreviation_required:
      "Set your DSP's abbreviation in its profile before collecting schedules.",
  },
};
