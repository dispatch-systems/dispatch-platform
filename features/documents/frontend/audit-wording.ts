import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Documents's events read in the platform owner's audit log: the Google account each
// connection event was about, and the file or folder each change was.

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
  'documents.created': (e, { strong }) => ['added ', strong(e.detail), ' to Documents'],
  'documents.renamed': (e, { strong }) => ['renamed ', strong(e.detail), ' in Documents'],
  'documents.trashed': (e, { strong }) => [
    'moved ',
    strong(e.detail),
    ' from Documents to the trash',
  ],
};

export const wording: AuditWording = {
  phrases,
  spoken: [
    'documents.connected',
    'documents.reconnected',
    'documents.disconnected',
    'documents.created',
    'documents.renamed',
    'documents.trashed',
  ],
  fields: { name: 'Name' },
};
