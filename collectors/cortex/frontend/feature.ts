import { createElement, lazy } from 'react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

const load = () => import('./index.js');
const CortexCard = lazy(() => load().then((module) => ({ default: module.CortexCard })));
const read = '/api/dsp/connections/cortex';

export const feature: FrontendFeature = {
  name: 'cortex',
  connectionCard: {
    provider: 'cortex',
    read,
    load,
    render: (context) => createElement(CortexCard, { ...context, read }),
  },
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
  errors: {
    cortex_station_unavailable:
      'Your saved station was not found in Cortex. Check your DSP profile and Cortex access.',
    cortex_provider_ambiguous:
      'Cortex could not identify your DSP. Check your DSP name and abbreviation.',
  },
  scheduleIssues: {
    schedule_meals_required: 'Connect Cortex before enabling Meal Break collections.',
    schedule_dvic_required: 'Connect Cortex before enabling DVIC collections.',
  },
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
