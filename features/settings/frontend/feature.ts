import { createElement, lazy } from 'react';
import { Settings } from 'lucide-react';
import type { Access, FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { prefetchSettings, preloadSettings } from './pages.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    settings: true;
  }
}

// The page, what the member has in Settings and the code of the page it opens on load
// together, so the page opens whole. Its sidebar is in the same code.
const load = (access?: Access) =>
  Promise.all([import('./index.js'), preloadSettings(access?.view)]).then(([module]) => module);
const SettingsPage = lazy(() => load().then((module) => ({ default: module.SettingsPage })));

export const feature: FrontendFeature = {
  name: 'settings',
  // Its route saves the profile a DSP's onboarding asks for.
  dspSetup: { permission: 'settings.manage', save: '/api/dsp/profile' },
  // Its pages and their search, in a sidebar of their own, which its item opens.
  sidebars: [
    {
      id: 'settings',
      scope: 'dsp',
      label: 'Settings',
      icon: Settings,
      draw: { load: () => import('./index.js').then((module) => module.SettingsSidebar) },
    },
  ],
  routes: [
    {
      id: 'settings',
      scope: 'dsp',
      label: 'Settings',
      icon: Settings,
      parent: 'settings',
      nav: true,
      hostsSettings: true,
      preload: load,
      render: ({ session, view }) => createElement(SettingsPage, { session, view }),
      prefetch: ({ view, immediate }) => prefetchSettings(view, immediate),
    },
  ],
};
