import { RadioTower } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Live Route Data's frontend manifest, loaded up front. It has no page of its own: Settings draws
// its section on its page there.

export const feature: FrontendFeature = {
  name: 'live_route_data',
  // The icon its page of Settings shows.
  icon: RadioTower,
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
  // Saving its section on Settings meets it, so the manifest words it.
  errors: {
    live_route_data_days_required: 'Choose at least one day.',
    live_route_data_window_invalid: 'Stop must be after start.',
  },
};
