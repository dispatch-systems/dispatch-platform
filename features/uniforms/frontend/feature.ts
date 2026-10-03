import { createElement, lazy } from 'react';
import { Shirt } from 'lucide-react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';
import { replies } from '../../../shared/contracts/runtime-uniforms.js';

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
  platformSlots: () => import('./platform-slots.js'),
  longPolls: ['/api/dsp/uniforms/updates'],
  replies,
  errors: {
    uniform_changed:
      'This uniform changed in another session. Close and reopen the editor before saving.',
    uniform_not_found: 'This uniform was removed. Refresh the inventory.',
    uniform_size_not_found: 'This size was removed or changed. Refresh the inventory.',
    uniform_name_taken: 'Another uniform already uses this name.',
    uniform_size_duplicate: 'Each fit can only have one entry for a size.',
    invalid_uniform_size: 'Size names must contain 1 to 24 characters.',
    uniform_size_in_stock: 'Remove the remaining stock before removing a size.',
    uniform_in_stock: 'Remove the remaining stock before archiving a uniform.',
    uniform_out_of_stock:
      'Another adjustment used the remaining stock. The current count has been refreshed.',
    uniform_quantity_limit: 'This size has reached the inventory limit.',
    uniform_inventory_initialized:
      'Inventory was already set up by another user. Refresh to see it.',
    uniform_limit: 'You can create up to 200 uniforms per DSP.',
    uniform_size_limit: 'A uniform can have up to 150 size and fit combinations.',
    uniform_request_conflict: 'This inventory request does not match its original adjustment.',
  },
  cache: {
    write: (write, url) =>
      write.startsWith('/api/dsp/uniforms') ? begins(url, '/api/dsp/uniforms') : undefined,
  },
};
