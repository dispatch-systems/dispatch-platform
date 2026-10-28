import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Communications' events read in the audit log: its phones', each by the phone's name and
// number.

const phrases: AuditPhrases = {
  'phones.paired': (e, { strong }) => ['paired phone ', strong(e.detail)],
  'phones.renamed': (e, { strong }) => ['renamed phone ', strong(e.detail)],
  'phones.removed': (e, { strong }) => ['removed phone ', strong(e.detail)],
};

export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
