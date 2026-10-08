import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Documents's events read in the platform owner's audit log. Each names the Google
// account it was about.

const phrases: AuditPhrases = {
  'documents.connected': (e, { strong }) => [
    'connected Google account ',
    strong(e.detail),
    ' to Documents',
  ],
  'documents.reconnected': (e, { strong }) => [
    'reconnected Google account ',
    strong(e.detail),
    ' to Documents',
  ],
  'documents.disconnected': (e, { strong }) => [
    'disconnected Google account ',
    strong(e.detail),
    ' from Documents',
  ],
  'documents.connection_broken': () => [
    "found that Google stopped accepting Documents' connection",
  ],
};

export const wording: AuditWording = {
  phrases,
  spoken: ['documents.connected', 'documents.reconnected', 'documents.disconnected'],
};
