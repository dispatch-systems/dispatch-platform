import { Route } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Routes has no screens: Settings draws its sections from the backend's declaration. It still
// fills the platform owner's slots: its switch's icon and how its events read in the audit log.
export const feature: FrontendFeature = {
  name: 'routes',
  // The icon of its page of Settings, as it has no page in the sidebar.
  icon: Route,
  platformSlots: () => import('./platform-slots.js'),
  collected: () => import('./collected.js'),
};
