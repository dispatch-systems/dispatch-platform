import type { ReactNode } from 'react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';
import type { DspPageContext, DspRouteId } from './pages.js';

// The page tabs slot: a tab one feature adds to another's page. A page with tabs of its own
// draws them among its own, and the app draws them above any other (`runtime/PageTabs.tsx`).
// One that draws nothing of its own is a link: it opens an outside address in a new browser tab.

type Placed = {
  /** The page it is a tab of. */
  page: DspRouteId;
  /** The tab's address on the page, `?tab=<id>`. It never changes, so links to it keep working. */
  id: string;
  label: string;
  /** Where the tab sits among the page's tabs, lowest first; a link comes after those drawn. */
  order: number;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
};

/** A tab drawn below the page's tab strip. */
export type DrawnTab = Placed & {
  /** Loads the tab's code. */
  load: () => Promise<unknown>;
  render: (context: DspPageContext) => ReactNode;
};

/** A tab that opens an outside address in a new browser tab, and has no address on the page. */
export type LinkTab = Placed & {
  /** A hook answering the address it opens, as the strip is drawn; with none, it isn't drawn. */
  useLink: (context: DspPageContext) => string | undefined;
};

/** A tab one feature adds to another's page. */
export type PageTab = DrawnTab | LinkTab;

export const isLinkTab = (tab: PageTab): tab is LinkTab => 'useLink' in tab;

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
