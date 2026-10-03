import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView, Feature, SessionView } from '../../../../shared/contracts/index.js';

// What an owner's `frontend/feature.ts` declares, and what the hosts read from it. Only
// app/frontend lists the manifests; it installs them here before the first render, so no host
// imports a feature.
//
// A manifest is loaded up front, and outside a browser by the route table's tests. It stays
// small, imports no CSS, touches no browser API until it is called, and loads its pages lazily.

/** Every page inside a DSP, by id. Each owner's feature.ts adds its own by declaration merging. */
export interface DspPages {}
/** Every page outside a DSP, by id, added the same way. */
export interface PlatformPages {}
export type DspRouteId = Extract<keyof DspPages, string>;
export type PlatformRouteId = Extract<keyof PlatformPages, string>;

/** Who is asking: the session, and inside a DSP its admitted view. */
export type Access = { session: SessionView; view?: DspView };
export type PageContext = { session: SessionView };
export type DspPageContext = PageContext & { view: DspView; reopen: () => Promise<void> };
/** What a page's prefetch hook may do: warm its primary data before the page mounts. */
export type RoutePrefetch = {
  session?: SessionView;
  /** Read now, at high priority, rather than queue a warmup. */
  immediate: boolean;
  /** Warms these reads for the page. */
  warm: (urls: string[]) => void;
};

type Page<Context> = {
  /** The page's name in the sidebar, the breadcrumb and the document title. */
  label: string;
  /** The navigation item to highlight for a page that has none of its own. */
  parent?: string;
  icon?: LucideIcon;
  /** Whether the sidebar lists the page. */
  nav: boolean | ((access: Access) => boolean);
  /** Who may open the page; omitted means everyone in the scope. */
  permission?: (access: Access) => boolean;
  /** The feature the page belongs to; a DSP without it has no such page. */
  feature?: Feature;
  /** Loads the page's code, and with access what the page opens on. */
  preload: (access?: Access) => Promise<unknown>;
  render: (context: Context) => ReactNode;
};
export type DspRoute = Page<DspPageContext> & {
  id: DspRouteId;
  scope: 'dsp';
  /** A page rendered before, whose admitted data is cached, may commit before the next paint. */
  ready?: (view: DspView) => boolean;
  /** Called only for the admitted view. */
  prefetch?: (prefetch: RoutePrefetch & { view: DspView }) => void;
};
export type PlatformRoute = Page<PageContext> & {
  id: PlatformRouteId;
  scope: 'platform';
  /** Called only while no DSP view is admitted. */
  prefetch?: (prefetch: RoutePrefetch) => void;
};
export type Route = DspRoute | PlatformRoute;

/** What a Settings panel is drawn for: the person, and inside a DSP its view. */
export type SettingsContext = { session: SessionView; view?: DspView };
/** A tab of a Settings page. */
export type SettingsTab = {
  /** The tab's address, `?tab=<id>`. It never changes, so links to it keep working. */
  id: string;
  label: string;
  /** Loads the panel's code. */
  load: () => Promise<unknown>;
  render: (context: SettingsContext) => ReactNode;
};

/** An owner's frontend: what it puts in each slot. */
export type FrontendFeature = {
  /** The owner's directory name. */
  name: string;
  /** Its pages, in sidebar order. */
  routes?: readonly Route[];
};

let installed: readonly FrontendFeature[] = [];
/** Called once by the app, before the first render, with every owner's manifest. */
export function installFeatures(features: readonly FrontendFeature[]) {
  installed = features;
}

/** The installed page at an address. */
export function routeOf(scope: 'dsp', page: string): DspRoute | undefined;
export function routeOf(scope: 'platform', page: string): PlatformRoute | undefined;
export function routeOf(scope: Route['scope'], page: string): Route | undefined {
  for (const feature of installed)
    for (const route of feature.routes ?? [])
      if (route.scope === scope && route.id === page) return route;
  return undefined;
}
