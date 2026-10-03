import { createElement, lazy } from 'react';
import { Shirt } from 'lucide-react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    uniforms: true;
  }
}

const load = () => import('./index.js');
const UniformInventoryPage = lazy(() =>
  load().then((module) => ({ default: module.UniformInventoryPage })),
);

export const feature: FrontendFeature = {
  name: 'uniforms',
  routes: [
    {
      id: 'uniforms',
      scope: 'dsp',
      label: 'Uniform Inventory',
      icon: Shirt,
      nav: true,
      feature: 'uniforms',
      permission: ({ view }) => can(view, 'uniforms.view'),
      preload: load,
      render: ({ view }) => createElement(UniformInventoryPage, { key: view.token, view }),
    },
  ],
  switch: { id: 'uniforms', icon: Shirt },
  cache: {
    write: (write, url) =>
      write.startsWith('/api/dsp/uniforms') ? begins(url, '/api/dsp/uniforms') : undefined,
  },
};
