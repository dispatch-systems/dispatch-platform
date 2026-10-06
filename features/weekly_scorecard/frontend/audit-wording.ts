import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

const requested: AuditPhrases[string] = (e, { strong }) => [
  'started a weekly scorecard collection',
  ...(e.detail ? [' for week ', strong(e.detail)] : []),
];

const phrases: AuditPhrases = {
  'weekly_scorecard.collection_requested': requested,
  'weekly_scorecard.policy_updated': () => ['updated the weekly scorecard collection policy'],
};
export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
