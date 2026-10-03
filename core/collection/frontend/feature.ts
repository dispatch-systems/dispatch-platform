import { createElement, lazy } from 'react';
import { connectionFeatures } from '../../shell/frontend/runtime/features.js';
import { can } from '../../shell/frontend/runtime/permissions.js';
import {
  connectionCard,
  connectionCards,
  type FrontendFeature,
} from '../../shell/frontend/runtime/slots.js';

// The tab loads with every collector's card, so it opens whole.
const load = () =>
  Promise.all([import('./index.js'), ...connectionCards().map((card) => card.load())]).then(
    ([module]) => module,
  );
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
          ? connectionFeatures(view.features).flatMap(
              (connection) => connectionCard(connection.id)?.read ?? [],
            )
          : [],
    },
  ],
};
