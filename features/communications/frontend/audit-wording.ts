import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Communications' events read in the audit log: its phones', each by the phone's name and
// number; the numbers that text the DSP, each by who it's with and its number; its quick replies;
// and the driver picked for a work phone, by their name and its number.

const phrases: AuditPhrases = {
  'phones.paired': (e, { strong }) => ['paired phone ', strong(e.detail)],
  'phones.renamed': (e, { strong }) => ['renamed phone ', strong(e.detail)],
  'phones.removed': (e, { strong }) => ['removed phone ', strong(e.detail)],
  'communications.contact_saved': (e, { strong }) => [
    'saved a name for ',
    strong(e.target ?? 'a number'),
  ],
  'communications.number_blocked': (e, { strong }) => ['blocked ', strong(e.detail)],
  'communications.number_unblocked': (e, { strong }) => ['unblocked ', strong(e.detail)],
  'communications.quick_replies_saved': () => ['changed the quick replies'],
  'communications.driver_picked': (e, { strong }) => [
    'picked ',
    strong(e.target ?? 'a driver'),
    ' for work phone ',
    strong(e.detail),
  ],
};

export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
