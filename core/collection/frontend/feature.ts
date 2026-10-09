import { createElement, lazy } from 'react';
import { connectionFeatures } from '../../shell/frontend/runtime/features.js';
import { can } from '../../shell/frontend/runtime/permissions.js';
import type { DspView } from '../../accounts/api/index.js';
import {
  connectionCard,
  connectionCards,
  connectionPieces,
  type FrontendFeature,
} from '../../shell/frontend/runtime/slots.js';

// The tab loads with every collector's card and every card features add, so it opens whole.
const load = () =>
  Promise.all([
    import('./index.js'),
    ...connectionCards().map((card) => card.load()),
    ...connectionPieces('dsp').map((piece) => piece.load()),
    ...connectionPieces('personal').map((piece) => piece.load()),
  ]).then(([module]) => module);
/**
 * Whether the member has anything on the tab: the DSP's accounts, which they manage, or one of
 * their own.
 */
const anything = (view?: DspView) =>
  can(view, 'connections.manage') || connectionPieces('personal', view).length > 0;
const ConnectionsTab = lazy(() => load().then((module) => ({ default: module.ConnectionsTab })));

export const feature: FrontendFeature = {
  name: 'collection',
  settingsTabs: [
    {
      id: 'connections',
      label: 'Connections',
      order: 30,
      visible: anything,
      load,
      render: ({ session, view }) => view && createElement(ConnectionsTab, { session, view }),
      prefetch: (tab, view) => {
        if (tab !== 'connections') return [];
        const manages = can(view, 'connections.manage');
        return [
          ...(manages
            ? connectionFeatures(view.features).flatMap(
                (connection) => connectionCard(connection.id)?.read ?? [],
              )
            : []),
          ...(manages ? connectionPieces('dsp', view) : []).flatMap((piece) => piece.reads),
          ...connectionPieces('personal', view).flatMap((piece) => piece.reads),
        ];
      },
    },
  ],
};
