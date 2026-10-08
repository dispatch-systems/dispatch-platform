import { createElement, lazy } from 'react';
import { FolderOpen } from 'lucide-react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { documentsOverviewUrl } from '../api/client.js';

// Documents's frontend manifest, loaded up front: it stays small and loads the rest lazily.

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    documents: true;
  }
}

const loadPage = () => import('./index.js');
const DocumentsPage = lazy(() => loadPage().then((module) => ({ default: module.DocumentsPage })));

export const feature: FrontendFeature = {
  name: 'documents',
  routes: [
    {
      id: 'documents',
      scope: 'dsp',
      label: 'Documents',
      icon: FolderOpen,
      nav: true,
      feature: 'documents',
      permission: ({ view }) => can(view, 'documents.use'),
      preload: loadPage,
      render: ({ view }) => createElement(DocumentsPage, { key: view.token, view }),
      prefetch: ({ view, warm }) => {
        if (can(view, 'documents.use')) warm([documentsOverviewUrl]);
      },
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
  errors: {
    google_unavailable: "Google isn't set up for Dispatch yet. Ask Dispatch support to finish it.",
    google_unreachable: "Google didn't answer. Try again in a minute.",
    google_sign_in_failed: "Google didn't finish signing in. Try connecting again.",
    google_drive_not_allowed:
      'Google wasn’t given access to Drive. Connect again, and leave the Drive box checked on Google’s screen.',
    google_email_unverified:
      "That Google account's email address isn't verified. Verify it with Google, or use another account.",
    documents_connect_expired: 'That sign-in took too long or was already used. Connect again.',
    documents_account_mismatch:
      'Reconnect with the Google account Documents was set up with. To use a different account, disconnect Google first.',
    google_connection_broken: 'Google stopped accepting the connection. Reconnect Google.',
  },
};
