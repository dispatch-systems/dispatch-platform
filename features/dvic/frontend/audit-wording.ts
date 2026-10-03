import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How DVIC's events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'dvic.collection_requested': (e, { strong }) => [
    'started a DVIC collection',
    ...(e.detail && e.detail !== 'recent' ? [' through publication week ', strong(e.detail)] : []),
  ],
};

export const wording: AuditWording = { phrases, spoken: ['dvic.collection_requested'] };
