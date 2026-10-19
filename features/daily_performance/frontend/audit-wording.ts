import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

const phrases: AuditPhrases = {
  'daily_performance.collection_requested': (e, { strong }) => [
    'started a daily performance collection',
    ...(e.detail ? [' for ', strong(e.detail)] : []),
  ],
  // Recorded before Settings kept the policy, which records its changes as `settings.changed`.
  'daily_performance.policy_updated': () => ['updated the daily performance collection policy'],
};
export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
