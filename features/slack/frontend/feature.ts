import { createElement, lazy } from 'react';
import { Blocks, Hash } from 'lucide-react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import {
  connectionIntegrations,
  type FrontendFeature,
} from '../../../core/shell/frontend/runtime/slots.js';
import {
  syncCanvases,
  syncCanvasesSection,
} from '../integrations/sync_canvases/frontend/integration.js';
import {
  importFiles,
  importFilesSection,
} from '../integrations/import_files/frontend/integration.js';
import { CHANNELS, INTEGRATIONS } from './sections.js';

// Slack's frontend manifest, loaded up front: it stays small and loads its cards and its section
// of Settings lazily. Its error codes are worded in its API client, which loads with them: only
// they and New make the calls that raise them.

const loadCards = () => import('./settings.js');
const SlackCards = lazy(() => loadCards().then((module) => ({ default: module.SlackCards })));
const NewSlack = lazy(() => loadCards().then((module) => ({ default: module.NewSlack })));
const SlackLogo = lazy(() => loadCards().then((module) => ({ default: module.SlackLogo })));
const loadPage = () => import('./page.js');
const Workspaces = lazy(() => loadPage().then((module) => ({ default: module.Workspaces })));
const ChannelsSection = lazy(() =>
  loadPage().then((module) => ({ default: module.ChannelsSection })),
);
const IntegrationsSection = lazy(() =>
  loadPage().then((module) => ({ default: module.IntegrationsSection })),
);

export const feature: FrontendFeature = {
  name: 'slack',
  // The icon of its page of Settings, as it has no page in the sidebar.
  icon: Hash,
  // Its workspaces' settings, its custom section of its page of Settings, for those who manage
  // Slack, reached from each workspace's card; its own page once was there.
  settingsSections: [
    {
      id: 'slack.workspaces',
      load: loadPage,
      render: ({ session, view }) => view && createElement(Workspaces, { session, view }),
      prefetch: () => ['/api/dsp/slack/account'],
    },
  ],
  movedToSettings: { slack: 'slack' },
  // Its workspaces are among the DSP's own accounts, each on a card of its own; New adds one.
  // Those who manage Slack see them too, for their settings.
  connectionPieces: [
    {
      id: 'slack',
      section: 'dsp',
      order: 20,
      others: (view) => can(view, 'slack.manage'),
      reads: ['/api/dsp/slack/account'],
      load: loadCards,
      render: ({ view }) => view && createElement(SlackCards, { view }),
      adds: {
        label: 'Slack',
        icon: () => createElement(SlackLogo, { size: 16 }),
        render: ({ close }) => createElement(NewSlack, { close }),
      },
    },
  ],
  // Its own sections of each workspace's settings, among which other features add theirs: the
  // channels Dispatch is in, and, while another feature works through Slack, the integrations
  // switched on there, which other features declare in `connectionIntegrations`.
  connectionSections: [
    {
      connection: 'slack',
      id: CHANNELS,
      label: 'Channels',
      icon: Hash,
      order: 10,
      render: ({ account }) => createElement(ChannelsSection, { team: account }),
    },
    {
      connection: 'slack',
      id: INTEGRATIONS,
      label: 'Integrations',
      icon: Blocks,
      order: 20,
      visible: (view) => Boolean(view && connectionIntegrations('slack', view).length),
      render: (context) => createElement(IntegrationsSection, context),
    },
    importFilesSection,
    syncCanvasesSection,
  ],
  // Its own integrations, beside those other features add.
  connectionIntegrations: [importFiles, syncCanvases],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
