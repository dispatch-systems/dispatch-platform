import { createElement, lazy } from 'react';
import { Clock, Image, Mail, MessageSquareOff, NotebookText, PenOff } from 'lucide-react';
import type { DspView } from '../../../../../core/accounts/api/index.js';
import { can } from '../../../../../core/shell/frontend/runtime/permissions.js';
import { offered } from '../../../../../core/shell/frontend/runtime/services.js';
import type {
  ConnectionIntegration,
  ConnectionSection,
} from '../../../../../core/shell/frontend/runtime/slots.js';

// Sync Canvases, Slack's own integration, as Slack's manifest declares it: its card under a
// workspace's Integrations, which says what it does for members who don't know, and its section,
// listed under Integrations while it's on. Both show only while something keeps the DSP's
// files (core's Files service), the section only to those who see where they're kept. Its
// screens load with the workspace's page.

const loadPage = () => import('../../../frontend/page.js');
const SyncCanvasesSection = lazy(() =>
  loadPage().then((module) => ({ default: module.SyncCanvasesSection })),
);
const CanvasSummary = lazy(() => loadPage().then((module) => ({ default: module.CanvasSummary })));

/** Its ID, as the backend declares it. */
export const SYNC_CANVASES = 'slack.sync_canvases';

/** Where the DSP keeps its files, while something does. */
const store = (view: DspView | undefined) => offered('files', view);

export const syncCanvases: ConnectionIntegration = {
  connection: 'slack',
  id: SYNC_CANVASES,
  label: 'Sync Canvases',
  icon: NotebookText,
  description:
    'Keeps canvases from Slack channels as documents in folders, updated whenever they’re edited.',
  byline: (view) => {
    const kept = store(view);
    return kept && `Saves to ${kept.name}`;
  },
  steps: [
    'Join the channel in Channels',
    'Link the channel, or choose its canvases, in Sync Canvases',
    'Each canvas is kept as a document there, updated within minutes of an edit',
  ],
  sees: [
    { icon: Image, text: 'Canvases in linked channels and those you choose, with their pictures' },
    { icon: Mail, text: "Members' email addresses, to show who made each canvas" },
    { icon: MessageSquareOff, text: 'Never messages' },
  ],
  turningOn: [
    { text: 'Canvases you link or choose are kept as documents, with their pictures' },
    { text: 'Each document updates within minutes of its canvas being edited' },
    { icon: PenOff, text: 'Changes made in a document are replaced by its canvas' },
    { text: "Dispatch reads members' email addresses to show who made each canvas" },
    { icon: MessageSquareOff, text: 'Messages are never saved' },
  ],
  turningOff: [
    { text: 'Documents already made stay where they are' },
    { icon: Clock, text: "They stop updating until it's turned back on" },
    { text: "Linked channels and chosen canvases are kept for when it's turned back on" },
  ],
  order: 20,
  visible: (view) => Boolean(store(view)),
  summary: ({ view, account }) => view && createElement(CanvasSummary, { view, team: account }),
};

export const syncCanvasesSection: ConnectionSection = {
  connection: 'slack',
  id: 'sync-canvases',
  label: 'Sync Canvases',
  icon: NotebookText,
  order: 40,
  integration: SYNC_CANVASES,
  visible: (view) => {
    const kept = store(view);
    return Boolean(kept && can(view, kept.permission));
  },
  render: (context) => createElement(SyncCanvasesSection, context),
};
