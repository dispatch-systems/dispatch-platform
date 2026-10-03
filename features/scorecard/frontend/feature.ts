import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Scorecard has no screens; agents read its data. Its events still read in the audit log, and
// its kinds of data show on the Agents page.
export const feature: FrontendFeature = {
  name: 'scorecard',
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
  readToggles: {
    label: 'Scorecard',
    missing: 'scorecard data',
    order: 50,
    sources: { scorecard: 'Scorecard' },
    toggles: [
      {
        id: 'feedback',
        label: 'Customer feedback',
        missing: 'customer feedback',
        source: 'scorecard',
      },
      { id: 'safety', label: 'Safety events', missing: 'safety events', source: 'scorecard' },
      {
        id: 'returns',
        label: 'Returns & contact compliance',
        missing: 'returns',
        source: 'scorecard',
      },
      {
        id: 'scorecard',
        label: 'Weekly scorecard',
        missing: 'weekly scorecard',
        source: 'scorecard',
      },
    ],
  },
};
