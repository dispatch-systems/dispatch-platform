import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Driver Match's events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'driver_match.merged': (e, { strong }) => [
    'confirmed ',
    strong(e.target ?? 'two people'),
    ' as one person',
  ],
  'driver_match.split': (e, { strong }) => [
    'split ',
    strong(e.target ?? 'a person'),
    ' off as their own person',
  ],
  'driver_match.kept_apart': (e, { strong }) => [
    'kept ',
    strong(e.target ?? 'two people'),
    ' apart',
  ],
  'driver_match.confirmed': (e, { strong }) => [
    'confirmed ',
    strong(e.target ?? 'a person'),
    "'s match",
  ],
  'driver_match.titled': (e, { strong }) => [
    'changed the title of ',
    strong(e.target ?? 'a person'),
  ],
};

export const wording: AuditWording = { phrases };
