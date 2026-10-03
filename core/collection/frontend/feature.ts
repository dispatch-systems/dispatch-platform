import { createElement, lazy } from 'react';
import { can } from '../../shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../shell/frontend/runtime/slots.js';

const load = () => import('./index.js');
const ConnectionsTab = lazy(() => load().then((module) => ({ default: module.ConnectionsTab })));

export const feature: FrontendFeature = {
  name: 'collection',
  settingsTabs: [
    {
      id: 'connections',
      label: 'Connections',
      order: 30,
      visible: (view) => can(view, 'connections.manage'),
      load,
      render: ({ session, view }) => view && createElement(ConnectionsTab, { session, view }),
      prefetch: (tab, view) =>
        tab === 'connections' && can(view, 'connections.manage')
          ? [
              ...(view.features.includes('paycom') ? ['/api/dsp/connections'] : []),
              ...(view.features.includes('cortex') ? ['/api/dsp/connections/cortex'] : []),
            ]
          : [],
    },
  ],
};
