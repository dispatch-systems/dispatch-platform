import { createElement, lazy } from 'react';
import { Clock, FolderInput, Image, Mail, MessageSquareOff } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { offered } from '../../../../core/shell/frontend/runtime/services.js';
import type {
  ConnectionIntegration,
  ConnectionSection,
} from '../../../../core/shell/frontend/runtime/slots.js';

// Import Files, Slack's own integration, as Slack's manifest declares it: its card under a
// workspace's Integrations, which says what it does for members who don't know, and its section,
// listed under Integrations while it's on. Both show only while something keeps the DSP's
// files (core's Files service), the section only to those who see where they're kept. Its
// screens load with the workspace's page.

const loadPage = () => import('../page.js');
const ImportFilesSection = lazy(() =>
  loadPage().then((module) => ({ default: module.ImportFilesSection })),
);
const ImportSummary = lazy(() => loadPage().then((module) => ({ default: module.ImportSummary })));

/** Its ID, as the backend declares it. */
export const IMPORT_FILES = 'slack.import_files';

/** Where the DSP keeps its files, while something does. */
const store = (view: DspView | undefined) => offered('files', view);

export const importFiles: ConnectionIntegration = {
  connection: 'slack',
  id: IMPORT_FILES,
  label: 'Import Files',
  icon: FolderInput,
  description:
    'Saves files and photos posted in Slack channels to folders. Messages are never saved.',
  byline: (view) => {
    const kept = store(view);
    return kept && `Saves to ${kept.name}`;
  },
  steps: [
    'Join the channel in Channels',
    'Link it to a folder in Import Files',
    'New files posted there, up to 100 MB each, are saved to that folder',
  ],
  sees: [
    { icon: Image, text: 'Files and photos posted in linked channels' },
    { icon: Mail, text: "Members' email addresses, to show who posted each file" },
    { icon: MessageSquareOff, text: 'Never messages' },
  ],
  turningOn: [
    { text: 'Files and photos posted in the channels you link are saved' },
    { text: 'Only new files, from when a channel is linked' },
    { text: "Dispatch reads members' email addresses to show who posted each file" },
    { icon: MessageSquareOff, text: 'Messages are never saved' },
  ],
  turningOff: [
    { text: 'Files already saved stay where they are' },
    { text: "Linked channels and their folders are kept for when it's turned back on" },
    { icon: Clock, text: "Files posted while it's off aren't saved" },
  ],
  order: 10,
  visible: (view) => Boolean(store(view)),
  summary: ({ view, account }) => view && createElement(ImportSummary, { view, team: account }),
};

export const importFilesSection: ConnectionSection = {
  connection: 'slack',
  id: 'import-files',
  label: 'Import Files',
  icon: FolderInput,
  order: 30,
  integration: IMPORT_FILES,
  visible: (view) => {
    const kept = store(view);
    return Boolean(kept && can(view, kept.permission));
  },
  render: (context) => createElement(ImportFilesSection, context),
};
