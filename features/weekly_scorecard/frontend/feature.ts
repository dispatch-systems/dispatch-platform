import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Weekly Scorecard has no screens; agents read its data. It still fills the platform owner's slots:
// its switch's icon, how its events read in the audit log and its kinds of data for agents.
export const feature: FrontendFeature = {
  name: 'weekly_scorecard',
  platformSlots: () => import('./platform-slots.js'),
};
