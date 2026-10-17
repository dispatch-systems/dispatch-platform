import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Slack's events read in the platform owner's audit log: the workspace each was about, the
// channel, `#<channel> in <workspace>`, the integration, `<integration> in <workspace>`, a
// channel Import Files or Sync Canvases links, `#<channel> to <folder> in <workspace>`, or a
// canvas chosen on its own, `<canvas> in #<channel> to <folder> in <workspace>`.

const phrases: AuditPhrases = {
  'slack.connected': (e, { strong }) => ['connected Slack workspace ', strong(e.detail)],
  'slack.connected_by_link': (e, { strong }) => [
    'Slack workspace ',
    strong(e.detail),
    ' was connected with the install link',
  ],
  'slack.reconnected': (e, { strong }) => ['reconnected Slack workspace ', strong(e.detail)],
  'slack.renamed': (e, { strong }) => ['renamed a Slack connection ', strong(e.detail)],
  'slack.disconnected': (e, { strong }) => ['disconnected Slack workspace ', strong(e.detail)],
  'slack.removed_in_slack': (e, { strong }) => [
    'Dispatch was removed from Slack workspace ',
    strong(e.detail),
    ' in Slack',
  ],
  'slack.connection_broken': (e, { strong }) => [
    'found that Slack stopped accepting the connection to ',
    strong(e.detail),
  ],
  'slack.channel_joined': (e, { strong }) => ['added Dispatch to Slack channel ', strong(e.detail)],
  'slack.channel_left': (e, { strong }) => [
    'took Dispatch out of Slack channel ',
    strong(e.detail),
  ],
  'slack.integration_on': (e, { strong }) => ['turned on Slack integration ', strong(e.detail)],
  'slack.integration_off': (e, { strong }) => ['turned off Slack integration ', strong(e.detail)],
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
  'slack.link_created': () => ['made a Slack install link'],
  'slack.link_cancelled': () => ['cancelled the Slack install link'],
};

export const wording: AuditWording = {
  phrases,
  spoken: [
    'slack.connected',
    'slack.reconnected',
    'slack.renamed',
    'slack.disconnected',
    'slack.channel_joined',
    'slack.channel_left',
    'slack.integration_on',
    'slack.integration_off',
    'slack.import_linked',
    'slack.import_moved',
    'slack.import_unlinked',
    'slack.canvases_linked',
    'slack.canvases_moved',
    'slack.canvases_unlinked',
    'slack.canvas_chosen',
    'slack.canvas_unchosen',
    'slack.link_created',
    'slack.link_cancelled',
  ],
};
