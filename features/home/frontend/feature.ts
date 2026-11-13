import { createElement, lazy } from 'react';
import { House } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import type { HomeLayout } from '../api/index.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    overview: true;
  }
}
declare module '../../../core/shell/frontend/runtime/preferences.js' {
  interface Preferences {
    'home.layout': HomeLayout;
  }
}

const load = () => import('./index.js');
const HomePage = lazy(() => load().then((module) => ({ default: module.HomePage })));

export const feature: FrontendFeature = {
  name: 'home',
  routes: [
    {
      id: 'overview',
      scope: 'dsp',
      label: 'Home Page',
      icon: House,
      nav: true,
      // A DSP opens here anyway.
      remembered: false,
      landing: true,
      preload: load,
      render: ({ session, view }) => createElement(HomePage, { session, view }),
    },
  ],
};
