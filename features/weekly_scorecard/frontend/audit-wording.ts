import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

const requested: AuditPhrases[string] = (e, { strong }) => [
  'started a weekly scorecard collection',
  ...(e.detail ? [' for week ', strong(e.detail)] : []),
];

const phrases: AuditPhrases = {
  'weekly_scorecard.collection_requested': requested,
  // Recorded before Settings kept the policy, which records its changes as `settings.changed`.
  'weekly_scorecard.policy_updated': () => ['updated the weekly scorecard collection policy'],
};
export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
