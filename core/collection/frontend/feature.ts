import { createElement, lazy } from 'react';
import { schedulingPermissions } from '../../tenancy/api/generated/access-catalog.js';
import { collectorsRead } from '../api/urls.js';
import { connectionFeatures } from '../../shell/frontend/runtime/features.js';
import { can } from '../../shell/frontend/runtime/permissions.js';
import {
  connectionCard,
  connectionCards,
  connectionPieces,
  type FrontendFeature,
} from '../../shell/frontend/runtime/slots.js';
import { dspPieces } from './dspPieces.js';

// The section loads with every collector's card and every card features add, so it opens whole.
const load = () =>
  Promise.all([
    import('./index.js'),
    ...connectionCards().map((card) => card.load()),
    ...connectionPieces('dsp').map((piece) => piece.load()),
    ...connectionPieces('personal').map((piece) => piece.load()),
  ]).then(([module]) => module);
const ConnectionsTab = lazy(() => load().then((module) => ({ default: module.ConnectionsTab })));
const loadCollectors = () => import('./collectors/index.js');
const CollectorsTab = lazy(() =>
  loadCollectors().then((module) => ({ default: module.CollectorsTab })),
);

export const feature: FrontendFeature = {
  name: 'collection',
  settingsSections: [
    {
      // Settings' Connections page: the DSP's accounts and the member's own.
      id: 'collection.connections',
      // Whether the member has anything there: the DSP's accounts, which they manage or a card
      // shows them, or one of their own.
      visible: (view) =>
        can(view, 'connections.manage') ||
        dspPieces(view).length > 0 ||
        connectionPieces('personal', view).length > 0,
      load,
      render: ({ session, view }) => view && createElement(ConnectionsTab, { session, view }),
      prefetch: (view) => [
        ...(can(view, 'connections.manage')
          ? connectionFeatures(view.features).flatMap(
              (connection) => connectionCard(connection.id)?.read ?? [],
            )
          : []),
        ...dspPieces(view).flatMap((piece) => piece.reads),
        ...connectionPieces('personal', view).flatMap((piece) => piece.reads),
      ],
    },
    {
      // Settings' Collectors page: when each of the DSP's collections runs, for those who set
      // when one does.
      id: 'collection.collectors',
      visible: (view) => schedulingPermissions.some((permission) => can(view, permission)),
      load: loadCollectors,
      render: ({ view }) => view && createElement(CollectorsTab, { view }),
      prefetch: () => [collectorsRead],
    },
  ],
  // Its collections' runs and their collectors' connections change it, as a save of a schedule
  // does any DSP read; a preview of one changes nothing.
  cache: {
    jobs: [collectorsRead],
    connections: [collectorsRead],
    write: (write) =>
      write.startsWith('/api/dsp/collectors/') && write.endsWith('/preview') ? false : undefined,
  },
};
