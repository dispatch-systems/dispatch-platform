import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Cortex puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  auditWording: wording,
  capabilities: {
    meal_breaks: 'a meal-break source',
    routes: 'a route source',
    dvic: 'a DVIC source',
    scorecard: 'a scorecard source',
  },
};
