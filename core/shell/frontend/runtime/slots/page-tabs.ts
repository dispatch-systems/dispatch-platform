import type { ReactNode } from 'react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';
import type { DspPageContext, DspRouteId } from './pages.js';

// The page tabs slot: a tab one feature adds to another's page, which that page draws among
// its own tabs, as Settings draws its settings tabs.

/** A tab one feature adds to another's page. */
export type PageTab = {
  /** The page it is a tab of. */
  page: DspRouteId;
  /** The tab's address on the page, `?tab=<id>`. It never changes, so links to it keep working. */
  id: string;
  label: string;
  /** Where the tab sits among the page's tabs, lowest first. */
  order: number;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** Loads the tab's code. */
  load: () => Promise<unknown>;
  render: (context: DspPageContext) => ReactNode;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Its tabs on another feature's page. */
    pageTabs?: readonly PageTab[];
  }
}

/** The tabs other owners add to `page` that the view has, in their order. */
export const pageTabs = (page: string, view: DspView) =>
  present(view, (feature) => feature.pageTabs)
    .filter((tab) => tab.page === page)
    .sort((a, b) => a.order - b.order);
