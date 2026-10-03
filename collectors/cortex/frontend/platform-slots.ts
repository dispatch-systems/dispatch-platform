import type { PlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { wording } from './audit-wording.js';

// What Cortex puts in the platform owner's slots, loaded with the platform owner's pages.
export const slots: PlatformSlots = {
  auditWording: wording,
  collections: [
    {
      kind: 'cortex.meal_breaks.collect',
      schedule: { id: 'meal_break', label: 'Meal breaks' },
      unit: 'itinerary',
      count: (metrics) => metrics.itineraries,
    },
    {
      kind: 'cortex.scorecard.collect',
      schedule: { id: 'scorecard', label: 'Scorecard' },
      unit: 'row',
      count: (metrics) => metrics.rows,
    },
    {
      kind: 'cortex.routes.collect',
      schedule: { id: 'routes', label: 'Routes' },
      unit: 'itinerary',
      count: (metrics) => metrics.itineraries,
    },
    {
      kind: 'cortex.dvic.collect',
      schedule: { id: 'dvic', label: 'DVIC' },
      unit: 'row',
      count: (metrics) => metrics.rows,
    },
  ],
};
