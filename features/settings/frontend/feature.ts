import { createElement, lazy } from 'react';
import { Settings } from 'lucide-react';
import type { Access, FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { prefetchSettings } from './prefetch.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    settings: true;
  }
}

const load = (access?: Access) =>
  import('./index.js').then(async (module) => {
    await module.preloadSettingsPage(access?.view);
    return module;
  });
const SettingsPage = lazy(() => load().then((module) => ({ default: module.SettingsPage })));

export const feature: FrontendFeature = {
  name: 'settings',
  routes: [
    {
      id: 'settings',
      scope: 'dsp',
      label: 'Settings',
      icon: Settings,
      nav: true,
      preload: load,
      render: ({ session, view }) => createElement(SettingsPage, { session, view }),
      prefetch: ({ view, immediate }) => prefetchSettings(view, immediate),
    },
  ],
};
