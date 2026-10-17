import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView, Permission, SessionView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { manifests } from './installed.js';

// The pages slot: an owner's pages, inside a DSP or outside, and which of them a DSP opens on,
// draws the Settings tabs, or saves a DSP's profile.

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
  /** The page's name in the sidebar and the document title. */
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
  /** Whether opening the DSP again returns here, as to the last page open; omitted, it does. */
  remembered?: boolean;
  /** Whether a DSP opens here when it has no last page to return to; one page at most does. */
  landing?: boolean;
  /** Whether the page draws every owner's settings tabs, as a DSP's Settings; one at most does. */
  hostsSettings?: boolean;
  /** Called only for the admitted view. */
  prefetch?: (prefetch: RoutePrefetch & { view: DspView }) => void;
};
export type PlatformRoute = Page<PageContext> & {
  id: PlatformRouteId;
  scope: 'platform';
  /** The page the sidebar lists it after, one an owner listed before its own declares. */
  after?: PlatformRouteId;
  /**
   * Whether signing in at a link to it, one with a query, stays there, as for an app asking
   * to connect; else signing in opens the platform's first page.
   */
  linked?: boolean;
  /** Called only while no DSP view is admitted. */
  prefetch?: (prefetch: RoutePrefetch) => void;
};
export type Route = DspRoute | PlatformRoute;

declare module './installed.js' {
  interface FrontendFeature {
    /** Its pages, in sidebar order. */
    routes?: readonly Route[];
    /**
     * Who may save a DSP's profile, and the route it serves that saves it, when the owner saves
     * it: opening a DSP that has none asks them for it. One owner at most says so; without one,
     * nobody is asked.
     */
    dspSetup?: { permission: Permission; save: string };
  }
}

/** The installed page at an address. */
export function routeOf(scope: 'dsp', page: string): DspRoute | undefined;
export function routeOf(scope: 'platform', page: string): PlatformRoute | undefined;
export function routeOf(scope: Route['scope'], page: string): Route | undefined {
  for (const feature of manifests())
    for (const route of feature.routes ?? [])
      if (route.scope === scope && route.id === page) return route;
  return undefined;
}

/** The DSP page that says so, the first listed if more do. */
function dspPage(says: (route: DspRoute) => boolean | undefined) {
  for (const feature of manifests())
    for (const route of feature.routes ?? [])
      if (route.scope === 'dsp' && says(route)) return route.id;
  return undefined;
}
/** The page a DSP opens on, as a feature declares it; none without one. */
export const landingPage = () => dspPage((route) => route.landing);
/** The page that draws every owner's settings tabs, as a feature declares it; none without one. */
export const settingsPage = () => dspPage((route) => route.hostsSettings);

/** Who may save a DSP's profile when it is first opened, and where, as the owner that saves it says. */
export const dspSetup = () => manifests().find((feature) => feature.dspSetup)?.dspSetup;
