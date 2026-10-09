import { createElement, lazy } from 'react';
import { FolderOpen } from 'lucide-react';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

// Documents's frontend manifest, loaded up front: it stays small and loads the rest lazily.
// Its error codes are worded in its API client, which loads with its page: only the page
// makes the calls that raise them.

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    documents: true;
  }
}

const loadPage = () => import('./index.js');
const DocumentsPage = lazy(() => loadPage().then((module) => ({ default: module.DocumentsPage })));
const loadCards = () => import('./settings.js');
const GoogleDriveCard = lazy(() =>
  loadCards().then((module) => ({ default: module.GoogleDriveCard })),
);
const GoogleAccountCard = lazy(() =>
  loadCards().then((module) => ({ default: module.GoogleAccountCard })),
);

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
        if (can(view, 'documents.use')) warm(['/api/dsp/documents']);
      },
    },
  ],
  // Its Google account is one of the DSP's own; each member's, their own.
  connectionPieces: [
    {
      id: 'google-drive',
      section: 'dsp',
      order: 10,
      reads: ['/api/dsp/documents/account'],
      load: loadCards,
      render: ({ view }) => view && createElement(GoogleDriveCard, { view }),
    },
    {
      id: 'google-account',
      section: 'personal',
      order: 10,
      // The platform owner isn't one of the DSP's members, so has no account of their own here:
      // only they are sent the DSP's roles, to look through one.
      visible: (view) => can(view, 'documents.use') && !view?.roles,
      reads: ['/api/dsp/documents'],
      load: loadCards,
      render: ({ session, view }) =>
        view && createElement(GoogleAccountCard, { view, email: session.user.email }),
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
