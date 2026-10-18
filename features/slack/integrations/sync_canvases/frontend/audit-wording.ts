import type { AuditPhrases } from '../../../../../core/shell/frontend/runtime/slots.js';

// How Sync Canvases' events read in the audit log: a channel it links,
// `#<channel> to <folder> in <workspace>`, or a canvas chosen on its own,
// `<canvas> in #<channel> to <folder> in <workspace>`.
export const phrases: AuditPhrases = {
  'slack.canvases_linked': (e, { strong }) => [
    'synced the canvases of Slack channel ',
    strong(e.detail),
  ],
  'slack.canvases_moved': (e, { strong }) => [
    'moved where the canvases of Slack channel ',
    strong(e.detail),
    ' are kept',
  ],
  'slack.canvases_unlinked': (e, { strong }) => [
    'stopped syncing the canvases of Slack channel ',
    strong(e.detail),
  ],
  'slack.canvas_chosen': (e, { strong }) => ['synced Slack canvas ', strong(e.detail)],
  'slack.canvas_unchosen': (e, { strong }) => ['stopped syncing Slack canvas ', strong(e.detail)],
};
