import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Team & Roles' own events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'invitation.revoked': (e, { strong }) => ['revoked the invitation for ', strong(e.detail)],
};

export const wording: AuditWording = { phrases, spoken: ['invitation.revoked'] };
