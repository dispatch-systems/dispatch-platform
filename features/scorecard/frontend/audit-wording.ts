import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Scorecard's events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'scorecard.collection_requested': (e, { strong }) => [
    'started a scorecard collection',
    ...(e.detail ? [' for week ', strong(e.detail)] : []),
  ],
};

export const wording: AuditWording = { phrases, spoken: ['scorecard.collection_requested'] };
