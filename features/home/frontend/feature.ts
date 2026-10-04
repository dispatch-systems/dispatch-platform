import { createElement, lazy } from 'react';
import { House } from 'lucide-react';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    overview: true;
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
      render: () => createElement(HomePage),
    },
  ],
};
