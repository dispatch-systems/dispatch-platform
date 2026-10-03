import { title } from '../../core/shell/frontend/lib/format.js';
import { features } from './features.js';

export type RouteMeta = {
  id: string;
  scope: 'dsp' | 'platform';
  label: string;
  /** The navigation item to highlight for a page that has none of its own. */
  parent?: string;
};

// Every page's address and label, as the owners' manifests declare them, without their
// components.
export const routeMeta: readonly RouteMeta[] = features.flatMap((feature) =>
  (feature.routes ?? []).map(({ id, scope, label, parent }) => ({ id, scope, label, parent })),
);

export const routeLabel = (scope: RouteMeta['scope'], page: string) =>
  routeMeta.find((route) => route.scope === scope && route.id === page)?.label ?? title(page);
