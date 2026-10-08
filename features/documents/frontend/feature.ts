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
  // Its switch's icon on the DSPs page, and how its events read in the audit log.
  platformSlots: () => import('./platform-slots.js'),
};
