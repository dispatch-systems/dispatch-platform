import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Scorecard has no screens; agents read its data. Its events still read in the audit log.
export const feature: FrontendFeature = {
  name: 'scorecard',
  auditWording: () => import('./audit-wording.js').then((module) => module.wording),
};
