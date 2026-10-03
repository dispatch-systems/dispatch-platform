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
};
