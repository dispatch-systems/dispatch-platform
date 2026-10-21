import { Award } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Weekly Scorecard has no screens of its own: Settings draws its settings from its backend, with
// its collection on the Collectors page. It gives the icon its collection shows there, and fills
// the platform owner's slots: its switch's icon and how its events read in the audit log.
export const feature: FrontendFeature = {
  name: 'weekly_scorecard',
  icon: Award,
  platformSlots: () => import('./platform-slots.js'),
};
