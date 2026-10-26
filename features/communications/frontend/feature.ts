import { createElement, lazy } from 'react';
import { MessagesSquare } from 'lucide-react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { messages } from '../integrations/messages/frontend/integration.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    communications: true;
  }
}

// Communications' frontend manifest, loaded up front: it stays small and loads the rest lazily.

const load = () => import('./index.js');
const CommunicationsPage = lazy(() =>
  load().then((module) => ({ default: module.CommunicationsPage })),
);
const loadDisplay = () => import('./display/DisplaySettings.js');
const DisplaySettings = lazy(() =>
  loadDisplay().then((module) => ({ default: module.DisplaySettings })),
);

export const feature: FrontendFeature = {
  name: 'communications',
  routes: [
    {
      id: 'communications',
      scope: 'dsp',
      label: 'Communications',
      icon: MessagesSquare,
      // How many conversations wait on an answer.
      badge: { load: () => import('./badge.js').then((module) => module.WaitingCount) },
      nav: true,
      // The list and the conversation open scroll within it.
      fills: true,
      feature: 'communications',
      permission: ({ view }) => can(view, 'communications.use'),
      preload: load,
      render: ({ session, view }) =>
        createElement(CommunicationsPage, { view, user: session.user.id }),
    },
  ],
  // How the page looks to each member, their own, on Settings' Display page.
  settingsSections: [
    {
      id: 'communications.display',
      load: loadDisplay,
      render: ({ view, refresh }) => view && createElement(DisplaySettings, { view, refresh }),
      prefetch: () => ['/api/dsp/settings'],
    },
  ],
  // Alerts the member to new messages on every page, as they chose in Message display.
  watchers: [
    {
      id: 'communications.alerts',
      visible: ({ view }) => can(view, 'communications.use'),
      load: () => import('./alerts/MessageAlerts.js').then((module) => module.MessageAlerts),
    },
  ],
  // Its switch's icon on the DSPs page.
  platformSlots: () => import('./platform-slots.js'),
  connectionIntegrations: [messages],
  // Its page waits on changes to the DSP's conversations, and who is typing in them.
  longPolls: ['/api/dsp/communications/updates'],
  // Each of its writes changes its reads, but saying who is typing, which changes none.
  cache: {
    write: (write, url) =>
      write.startsWith('/api/dsp/communications') && !write.endsWith('/typing')
        ? begins(url, '/api/dsp/communications')
        : undefined,
  },
};
