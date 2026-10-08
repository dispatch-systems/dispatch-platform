import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Documents's events read in the platform owner's audit log: the Google account each
// connection event was about, the file or folder each change was, and who the folder was
// shared with.

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
  'documents.linked': (e, { strong }) => [
    'linked Google account ',
    strong(e.detail),
    ' to edit in Documents',
  ],
  'documents.unshared': (e, { strong }) => [
    'removed ',
    strong(e.detail),
    ' from the Documents folder in Google',
  ],
  'documents.created': (e, { strong }) => ['added ', strong(e.detail), ' to Documents'],
  'documents.uploaded': (e, { strong }) => ['uploaded ', strong(e.detail), ' to Documents'],
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
    'documents.linked',
    'documents.unshared',
    'documents.created',
    'documents.uploaded',
    'documents.renamed',
    'documents.trashed',
  ],
  fields: { name: 'Name' },
};
