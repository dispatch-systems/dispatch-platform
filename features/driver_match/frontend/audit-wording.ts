import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Driver Match's events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'driver_match.merged': (e, { strong }) => [
    'confirmed ',
    strong(e.target ?? 'two drivers'),
    ' as one person',
  ],
  'driver_match.split': (e, { strong }) => [
    'split ',
    strong(e.target ?? 'a driver'),
    ' off as their own person',
  ],
  'driver_match.kept_apart': (e, { strong }) => [
    'kept ',
    strong(e.target ?? 'two drivers'),
    ' apart',
  ],
};

export const wording: AuditWording = { phrases };
