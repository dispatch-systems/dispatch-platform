import { createElement, lazy } from 'react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

const load = () => import('./index.js');
const PaycomCard = lazy(() => load().then((module) => ({ default: module.PaycomCard })));
// Paycom's connection keeps the address it had before there were others.
const read = '/api/dsp/connections';

export const feature: FrontendFeature = {
  name: 'paycom',
  connectionCard: {
    provider: 'paycom',
    read,
    load,
    render: (context) => createElement(PaycomCard, { ...context, read }),
  },
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
  collections: [
    {
      kind: 'paycom.collect',
      schedule: { id: 'paycom', label: 'Paycom' },
      unit: 'employee',
      count: (metrics) => metrics.employees,
    },
  ],
};
