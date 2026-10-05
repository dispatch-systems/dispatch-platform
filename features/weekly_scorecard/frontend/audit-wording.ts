import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

const requested: AuditPhrases[string] = (e, { strong }) => [
  'started a weekly scorecard collection',
  ...(e.detail ? [' for week ', strong(e.detail)] : []),
];

// Historical audit entries retain their original action; both read the same way.
const phrases: AuditPhrases = {
  'scorecard.collection_requested': requested,
  'weekly_scorecard.collection_requested': requested,
};
export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
