import { createElement, lazy } from 'react';
import { Settings } from 'lucide-react';
import type { Access, FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { prefetchSettings, preloadSettingsTab } from './tabs.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    settings: true;
  }
}

// The page and the tab it opens on load together, so the page opens whole.
const load = (access?: Access) =>
  Promise.all([import('./index.js'), preloadSettingsTab(access?.view)]).then(([module]) => module);
const SettingsPage = lazy(() => load().then((module) => ({ default: module.SettingsPage })));

export const feature: FrontendFeature = {
  name: 'settings',
  // Its route saves the profile a DSP's onboarding asks for.
  dspSetup: { permission: 'settings.manage', save: '/api/dsp/profile' },
  routes: [
    {
      id: 'settings',
      scope: 'dsp',
      label: 'Settings',
      icon: Settings,
      nav: true,
      hostsSettings: true,
      preload: load,
      render: ({ session, view }) => createElement(SettingsPage, { session, view }),
      prefetch: ({ view, immediate }) => prefetchSettings(view, immediate),
    },
  ],
};
