import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Weekly Scorecard has no screens. It still fills the platform owner's slots, its switch's icon
// and how its events read in the audit log, and gives its data's icon to the lists of people.
export const feature: FrontendFeature = {
  name: 'weekly_scorecard',
  platformSlots: () => import('./platform-slots.js'),
  people: () => import('./people.js'),
};
