import { LayoutGrid } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Daily Performance's frontend manifest, loaded up front: it stays small and loads the rest lazily.

export const feature: FrontendFeature = {
  name: 'daily_performance',
  // The icon its collection shows on Settings' Collectors page, as it has no page in the sidebar.
  icon: LayoutGrid,
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
};
