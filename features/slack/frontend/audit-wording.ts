import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';
import { phrases as importFiles } from '../integrations/import_files/frontend/audit-wording.js';
import { phrases as syncCanvases } from '../integrations/sync_canvases/frontend/audit-wording.js';

// How Slack's events read in the platform owner's audit log: the workspace each was about, the
// channel, `#<channel> in <workspace>`, or the integration, `<integration> in <workspace>`. Each
// of its integrations words its own, in its folder.

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
  'slack.link_created': (e, { strong }) =>
    e.detail ? ['made a Slack install link for ', strong(e.detail)] : ['made a Slack install link'],
  'slack.link_cancelled': () => ['cancelled the Slack install link'],
};

export const wording: AuditWording = {
  phrases: { ...phrases, ...importFiles, ...syncCanvases },
  spoken: [
    'slack.connected',
    'slack.reconnected',
    'slack.renamed',
    'slack.disconnected',
    'slack.channel_joined',
    'slack.channel_left',
    'slack.integration_on',
    'slack.integration_off',
    ...Object.keys(importFiles),
    ...Object.keys(syncCanvases),
    'slack.link_created',
    'slack.link_cancelled',
  ],
};
