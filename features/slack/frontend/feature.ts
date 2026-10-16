import { createElement, lazy } from 'react';
import { Hash } from 'lucide-react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { settingsPage, type FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Slack's frontend manifest, loaded up front: it stays small and loads its cards and its page
// lazily. Its error codes are worded in its API client, which loads with them: only they and New
// make the calls that raise them.

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    slack: true;
  }
}

const loadCards = () => import('./settings.js');
const SlackCards = lazy(() => loadCards().then((module) => ({ default: module.SlackCards })));
const NewSlack = lazy(() => loadCards().then((module) => ({ default: module.NewSlack })));
const SlackLogo = lazy(() => loadCards().then((module) => ({ default: module.SlackLogo })));
const loadPage = () => import('./page.js');
const WorkspacePage = lazy(() => loadPage().then((module) => ({ default: module.WorkspacePage })));
const ChannelsSection = lazy(() =>
  loadPage().then((module) => ({ default: module.ChannelsSection })),
);

export const feature: FrontendFeature = {
  name: 'slack',
  // A workspace's settings, a page of their own reached from its card, for those who manage
  // Slack. Settings stays lit in the sidebar while it's open, wherever Settings is.
  routes: [
    {
      id: 'slack',
      scope: 'dsp',
      label: 'Slack',
      get parent() {
        return settingsPage();
      },
      nav: false,
      remembered: false,
      feature: 'slack',
      permission: ({ view }) => can(view, 'slack.manage'),
      preload: loadPage,
      render: ({ session, view }) => createElement(WorkspacePage, { session, view }),
    },
  ],
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
  // Its own section of each workspace's settings, where other features add theirs.
  connectionSections: [
    {
      connection: 'slack',
      id: 'channels',
      label: 'Channels',
      icon: Hash,
      order: 10,
      render: ({ account }) => createElement(ChannelsSection, { team: account }),
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
