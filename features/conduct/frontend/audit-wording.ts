import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Conduct's events read in the activity log: the category each change was to, and the
// writeup deleted, by who it was for, its category and its date.

const phrases: AuditPhrases = {
  'conduct.category_added': (e, { strong }) => ['added the Conduct category ', strong(e.detail)],
  'conduct.category_changed': (e, { strong }) => [
    'changed the Conduct category ',
    strong(e.detail),
  ],
  'conduct.category_retired': (e, { strong }) => [
    'retired the Conduct category ',
    strong(e.detail),
  ],
  'conduct.category_restored': (e, { strong }) => [
    'restored the Conduct category ',
    strong(e.detail),
  ],
  'conduct.writeup_deleted': (e, { strong }) => ['deleted the writeup ', strong(e.detail)],
};

// Each sentence names its event's detail itself.
export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
