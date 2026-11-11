import { createElement, lazy } from 'react';
import { MessagesSquare } from 'lucide-react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { messages } from '../integrations/messages/frontend/integration.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    'dispatch-phone': true;
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
const loadQuickReplies = () => import('./replies/QuickRepliesSettings.js');
const QuickRepliesSettings = lazy(() =>
  loadQuickReplies().then((module) => ({ default: module.QuickRepliesSettings })),
);
const loadPhones = () => import('./texting/phone/PhonesCard.js');
const PhonesCard = lazy(() => loadPhones().then((module) => ({ default: module.PhonesCard })));

const addresses = ['/api/dsp/dispatch-phone', '/api/dsp/communications'];
const here = (path: string) => addresses.some((base) => begins(path, base));

export const feature: FrontendFeature = {
  name: 'communications',
  routes: [
    {
      id: 'dispatch-phone',
      formerly: ['communications'],
      scope: 'dsp',
      label: 'Dispatch Phone',
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
  // How the page looks to each member, their own, on Settings' Display page; and the DSP's
  // quick replies, on its own page.
  settingsSections: [
    {
      id: 'communications.display',
      load: loadDisplay,
      render: ({ view, refresh }) => view && createElement(DisplaySettings, { view, refresh }),
      prefetch: () => ['/api/dsp/settings'],
    },
    {
      id: 'communications.quick_replies',
      load: loadQuickReplies,
      render: () => createElement(QuickRepliesSettings),
      prefetch: () => ['/api/dsp/dispatch-phone/quick-replies'],
    },
  ],
  // Alerts the member to new messages on every page, as they chose in Message display, and a
  // banner atop every page while one of the DSP's phones is down.
  watchers: [
    {
      id: 'communications.alerts',
      visible: ({ view }) => can(view, 'communications.use'),
      load: () => import('./alerts/MessageAlerts.js').then((module) => module.MessageAlerts),
    },
    {
      id: 'communications.phones',
      visible: ({ view }) => can(view, 'communications.use'),
      load: () => import('./texting/PhonesDown.js').then((module) => module.PhonesDown),
    },
  ],
  // The DSP's dispatch phones, on Settings' Connections, for those who manage them or all its
  // connections.
  connectionPieces: [
    {
      id: 'phones',
      section: 'dsp',
      order: 100,
      visible: (view) => can(view, 'connections.manage') || can(view, 'phones.manage'),
      // Managing the DSP's phones alone shows it, as managing all its connections does.
      others: (view) => can(view, 'phones.manage'),
      reads: ['/api/dsp/dispatch-phone/phones'],
      load: loadPhones,
      render: ({ view }) => view && createElement(PhonesCard, { view }),
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
  connectionIntegrations: [messages],
  // Its page waits on changes to the DSP's conversations, and who is typing in them.
  longPolls: ['/api/dsp/dispatch-phone/updates'],
  // Each of its writes changes its reads, but saying who is typing, which changes none.
  cache: {
    write: (write, url) => (here(write) && !write.endsWith('/typing') ? here(url) : undefined),
  },
};
