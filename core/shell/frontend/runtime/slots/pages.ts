import type { ComponentType, ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { DspView, Permission, SessionView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { manifests } from './installed.js';

// The pages slot: an owner's pages, inside a DSP or outside, and which of them a DSP opens on,
// draws Settings, or saves a DSP's profile, and the sidebars of pages that list some of them.

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
  /**
   * The item it is under: a page, highlighted while this one is open, or a sidebar of pages
   * (`sidebars`), which lists it in place of the app's. A page under a page of a sidebar's, as
   * a page of detail is, shows that sidebar too.
   */
  parent?: string;
  icon?: LucideIcon;
  /**
   * Drawn beside its name in the sidebar, as a count of what waits there; `active` while the
   * page is open. It loads on its own once the sidebar is drawn, which never waits for it.
   */
  badge?: { load: () => Promise<ComponentType<{ active: boolean }>> };
  /** Whether a sidebar lists the page: the app's, or the sidebar of pages it is under. */
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
  /**
   * The addresses it was once at, each a page and its query, as `settings?tab=x` for a tab of
   * Settings it was: a link to one opens this page. Each is kept as long as such links last.
   */
  formerly?: readonly string[];
  /** Whether a DSP opens here when it has no last page to return to; one page at most does. */
  landing?: boolean;
  /**
   * Whether it fills the window below the app's bars, its parts scrolling within it, as a
   * conversation does, so the page itself never scrolls.
   */
  fills?: boolean;
  /** Whether the page draws a DSP's Settings, every owner's sections of it; one at most does. */
  hostsSettings?: boolean;
  /**
   * Draws the tabs other owners add to it among its own, with `usePageTabs`; the app draws
   * them above any other page.
   */
  tabs?: boolean;
  /** Called only for the admitted view. */
  prefetch?: (prefetch: RoutePrefetch & { view: DspView }) => void;
};
/**
 * A sidebar of pages, an owner's own: an item of the app's sidebar, where the first of its pages
 * would be listed, that slides in a list of the pages under it (each naming it its `parent`) in
 * place of the app's, the page open staying open. Opening one of them opens it; its way back
 * slides the app's sidebar in again. The shell draws the list, the way back and its name atop
 * it, unless it draws its own (`draw`), as Settings draws its search and its groups. A page
 * under it that it doesn't list (`nav`), as one a link opens, still shows it as it opens.
 */
export type Sidebar = {
  /** Its id, which the pages under it name as their `parent`. */
  id: string;
  scope: Route['scope'];
  /** Its name, on its item and atop its list. */
  label: string;
  icon?: LucideIcon;
  /**
   * What it draws in place of the shell's list: `SidebarHead` atop it, with its name and
   * `back`, and what it lists. It fills the sidebar above the account, scrolling its own parts.
   * Sliding out, it keeps the props it had, and what it reads elsewhere should hold too, as
   * Settings keeps the page the address named. Its code loads as its item is pointed at, or as
   * a page under it opens.
   */
  draw?: { load: () => Promise<ComponentType<SidebarProps>> };
};
/** What a sidebar of pages that draws its own is drawn with. */
export type SidebarProps = Access & {
  /**
   * The DSP open, whether or not its view is admitted yet, for links drawn before it is; none
   * outside one.
   */
  dspId?: string;
  /** The page open, under the sidebar or not: opening the sidebar keeps it open. */
  page: string;
  /** Slides the app's sidebar in again, keeping the page open: what its way back does. */
  back: () => void;
  /** Closes the sidebar where it opens over the page, on a narrow screen, as a link in it does. */
  close: () => void;
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
    /** Its sidebars of pages, each an item of the app's sidebar listing the pages under it. */
    sidebars?: readonly Sidebar[];
    /**
     * Its icon where none of its pages shows one, as for its page of Settings when it has no
     * page in the sidebar.
     */
    icon?: LucideIcon;
    /**
     * Who may save a DSP's profile, and the route it serves that saves it, when the owner saves
     * it: opening a DSP that has none asks them for it. One owner at most says so; without one,
     * nobody is asked.
     */
    dspSetup?: { permission: Permission; save: string };
    /**
     * Its DSP pages that are pages of Settings now, each by its id with the page of Settings
     * it is (`?tab=`): a link to one opens that page, with the rest of the link's query.
     */
    movedToSettings?: Readonly<Record<string, string>>;
  }
}

/** The installed page at an address. */
export function routeOf(scope: 'dsp', page: string): DspRoute | undefined;
export function routeOf(scope: 'platform', page: string): PlatformRoute | undefined;
export function routeOf(scope: Route['scope'], page: string): Route | undefined;
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
/** The page that draws a DSP's Settings, as a feature declares it; none without one. */
export const settingsPage = () => dspPage((route) => route.hostsSettings);
/**
 * Where a link to `page` with `query` leads now, if a page was once there: the DSP page, and
 * the query it opens with, for a page of Settings its `tab` and the rest of the link's.
 */
export function movedTo(
  page: string,
  query: URLSearchParams,
): { page: DspRouteId; query?: Record<string, string> } | undefined {
  const formerly = dspPage((route) =>
    route.formerly?.some((address) => {
      const [at, asked] = address.split('?');
      return (
        at === page &&
        [...new URLSearchParams(asked)].every(([key, value]) => query.get(key) === value)
      );
    }),
  );
  if (formerly) return { page: formerly };
  const host = settingsPage();
  const tab = manifests().find((feature) => feature.movedToSettings?.[page])?.movedToSettings?.[
    page
  ];
  if (!host || !tab) return undefined;
  const rest = [...query].filter(([key]) => key !== 'tab');
  return { page: host, query: Object.fromEntries([['tab', tab], ...rest]) };
}

/** The installed sidebar of pages with the id. */
function sidebarNamed(scope: Route['scope'], id: string) {
  for (const feature of manifests())
    for (const sidebar of feature.sidebars ?? [])
      if (sidebar.scope === scope && sidebar.id === id) return sidebar;
  return undefined;
}
/**
 * The sidebar of pages `page` is under, if it is under one: its `parent`, or its parent's, as a
 * page of detail is under a page of the sidebar's.
 */
export function pageSidebar(scope: Route['scope'], page: string) {
  const parent = routeOf(scope, page)?.parent;
  if (!parent) return undefined;
  const above = routeOf(scope, parent)?.parent;
  return sidebarNamed(scope, parent) ?? (above ? sidebarNamed(scope, above) : undefined);
}
/** Whether a sidebar lists the page for `access`: the app's, or the one it is under. */
export const listed = (route: Route, access: Access) =>
  (typeof route.nav === 'function' ? route.nav(access) : route.nav) &&
  (!route.permission || route.permission(access));
/**
 * The items of the app's sidebar for `access`, from `pages` in their order: each page it lists,
 * but those under a sidebar of pages, which is listed in their place, once, where the first is,
 * with them. A sidebar none of whose pages it lists is left out.
 */
export function sidebarItems<P extends Route>(
  scope: Route['scope'],
  access: Access,
  pages: readonly P[],
) {
  type Listing = { sidebar: Sidebar; pages: P[] };
  const items: (P | Listing)[] = [];
  for (const page of pages) {
    if (page.scope !== scope || !listed(page, access)) continue;
    const sidebar = pageSidebar(scope, page.id);
    const there =
      sidebar &&
      items.find((item): item is Listing => 'sidebar' in item && item.sidebar === sidebar);
    if (!sidebar) items.push(page);
    else if (there) there.pages.push(page);
    else items.push({ sidebar, pages: [page] });
  }
  return items;
}
type Drawing = NonNullable<Sidebar['draw']>;
const drawings = new WeakMap<Drawing, ComponentType<SidebarProps>>();
/**
 * Loads what a sidebar of pages draws of its own; once in, `loadedSidebar` answers it, so the
 * sidebar draws as it slides in.
 */
export const loadSidebar = (drawing: Drawing) =>
  drawing.load().then((drawn) => {
    drawings.set(drawing, drawn);
    return drawn;
  });
export const loadedSidebar = (drawing: Drawing) => drawings.get(drawing);

/**
 * A feature's icon: that of the first of its DSP pages with one in the sidebar, else the one its
 * manifest gives (its switch's id is its owner's name).
 */
export function featureIcon(feature: string) {
  for (const owner of manifests())
    for (const route of owner.routes ?? [])
      if (route.scope === 'dsp' && route.feature === feature && route.icon) return route.icon;
  return manifests().find((owner) => owner.name === feature)?.icon;
}

/** Who may save a DSP's profile when it is first opened, and where, as the owner that saves it says. */
export const dspSetup = () => manifests().find((feature) => feature.dspSetup)?.dspSetup;
