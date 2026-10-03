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
  platformSlots: () => import('./platform-slots.js'),
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
};
