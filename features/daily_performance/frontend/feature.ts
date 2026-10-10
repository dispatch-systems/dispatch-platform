import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Daily Performance's frontend manifest, loaded up front: it stays small and loads the rest lazily.

export const feature: FrontendFeature = {
  name: 'daily_performance',
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
};
