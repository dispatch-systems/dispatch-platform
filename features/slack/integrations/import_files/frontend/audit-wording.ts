import type { AuditPhrases } from '../../../../../core/shell/frontend/runtime/slots.js';

// How Import Files' events read in the audit log: a channel it links,
// `#<channel> to <folder> in <workspace>`.
export const phrases: AuditPhrases = {
  'slack.import_linked': (e, { strong }) => ['linked Slack channel ', strong(e.detail)],
  'slack.import_moved': (e, { strong }) => [
    'moved where Slack channel ',
    strong(e.detail),
    ' saves',
  ],
  'slack.import_unlinked': (e, { strong }) => [
    'stopped importing Slack channel ',
    strong(e.detail),
  ],
};
