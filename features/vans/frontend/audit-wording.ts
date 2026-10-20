import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Vans' events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'vans.collection_requested': () => ['started a van collection'],
};

export const wording: AuditWording = {
  phrases,
  spoken: ['vans.collection_requested'],
};
