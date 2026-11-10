import { createElement, lazy } from 'react';
import { FileWarning } from 'lucide-react';
import { offered } from '../../../core/shell/frontend/runtime/exposes.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { backupUrl, conductUrl } from '../api/client.js';

// Conduct's frontend manifest, loaded up front: it stays small and loads the rest lazily.

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    conduct: true;
  }
}

const load = () => import('./index.js');
const ConductPage = lazy(() => load().then((module) => ({ default: module.ConductPage })));
const CategoriesSection = lazy(() =>
  load().then((module) => ({ default: module.CategoriesSection })),
);
const BackupSection = lazy(() => load().then((module) => ({ default: module.BackupSection })));

export const feature: FrontendFeature = {
  name: 'conduct',
  routes: [
    {
      id: 'conduct',
      scope: 'dsp',
      label: 'Conduct',
      icon: FileWarning,
      nav: true,
      feature: 'conduct',
      permission: ({ view }) => can(view, 'conduct.use'),
      preload: load,
      render: ({ session, view }) => createElement(ConductPage, { key: view.token, session, view }),
      prefetch: ({ view, warm }) => {
        if (can(view, 'conduct.use')) warm([conductUrl]);
      },
    },
  ],
  // On its page of Settings: its categories, beside how long a writeup counts, and backing
  // signed copies up, only while something keeps the DSP's files (what a feature exposes as
  // `files`).
  settingsSections: [
    {
      id: 'conduct.categories',
      load,
      render: () => createElement(CategoriesSection),
      prefetch: () => [conductUrl],
    },
    {
      id: 'conduct.backup',
      load,
      render: ({ view, refresh }) => view && createElement(BackupSection, { view, refresh }),
      visible: (view) => Boolean(offered('files', view)),
      prefetch: (view) => (offered('files', view) ? [backupUrl] : []),
    },
  ],
  // Its switch's icon on the DSPs page, and how its events read in the activity log.
  platformSlots: () => import('./platform-slots.js'),
};
