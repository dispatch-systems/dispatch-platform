import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Driver Profiles' events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'driver_profiles.collection_requested': () => ['started a roster collection'],
};

export const wording: AuditWording = {
  phrases,
  spoken: ['driver_profiles.collection_requested'],
};
