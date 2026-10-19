import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Routes has no screens: Settings draws its section from the backend's declaration. It still
// fills the platform owner's slots: its switch's icon and how its events read in the audit log.
export const feature: FrontendFeature = {
  name: 'routes',
  platformSlots: () => import('./platform-slots.js'),
};
