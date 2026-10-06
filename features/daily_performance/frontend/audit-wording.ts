import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

const phrases: AuditPhrases = {
  'daily_performance.collection_requested': (e, { strong }) => [
    'started a daily performance collection',
    ...(e.detail ? [' for ', strong(e.detail)] : []),
  ],
  'daily_performance.policy_updated': () => ['updated the daily performance collection policy'],
};
export const wording: AuditWording = { phrases, spoken: Object.keys(phrases) };
