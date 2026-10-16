import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Slack's events read in the platform owner's audit log: the workspace each was about.

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
    'slack.link_created',
    'slack.link_cancelled',
  ],
};
