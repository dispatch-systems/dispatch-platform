import { Suspense } from 'react';
import type { DspView } from '../../core/accounts/api/index.js';
import { Empty } from '../../core/shell/frontend/ui/Empty.js';
import { ErrorBox } from '../../core/shell/frontend/ui/ErrorBox.js';
import { Loading } from '../../core/shell/frontend/ui/Loading.js';
import { PageBoundary } from '../../core/shell/frontend/ui/PageBoundary.js';
import { OpenPage, PageWithTabs } from '../../core/shell/frontend/runtime/PageTabs.js';
import { hasFeature } from '../../core/shell/frontend/runtime/features.js';
import { NavigationStateContext } from '../../core/shell/frontend/runtime/browser-update.js';
import { prefetchRouteData } from '../../core/shell/frontend/runtime/route-prefetch.js';
import {
  landingPage,
  listed,
  loadSidebar,
  pageSidebar,
  sidebarItems,
  type Access,
  type DspRoute,
  type DspRouteId,
  type PageContext,
  type Route,
} from '../../core/shell/frontend/runtime/slots.js';
import { dspHash } from '../../core/shell/frontend/runtime/navigation.js';
import type { NavItem } from '../../core/shell/frontend/shell/Shell.js';
import { features } from './features.js';

// Every owner's pages, each with its navigation, access and component, in sidebar order: its
// owner's, but a platform page that names another to follow comes right after it.
const table: readonly Route[] = placed(features.flatMap((feature) => feature.routes ?? []));
function placed(routes: readonly Route[]) {
  const listed = routes.filter((route) => route.scope !== 'platform' || !route.after);
  for (const route of routes)
    if (route.scope === 'platform' && route.after) {
      const at = listed.findIndex(
        (other) => other.scope === 'platform' && other.id === route.after,
      );
      listed.splice(at < 0 ? listed.length : at + 1, 0, route);
    }
  return listed;
}
const allowed = (route: Route, access: Access) => !route.permission || route.permission(access);
/** Whether the DSP open has the page's feature, as far as its view says yet. */
const available = (route: Route, access: Access) =>
  route.scope !== 'dsp' || !route.feature || !access.view || hasFeature(access.view, route.feature);

export const findRoute = (scope: Route['scope'], page: string) =>
  table.find((route) => route.scope === scope && route.id === page);
/** A previously rendered tab with admitted cached data can commit before the next paint. */
export function canNavigateImmediately(scope: Route['scope'], page: string, access: Access) {
  const route = findRoute(scope, page);
  return Boolean(route?.scope === 'dsp' && access.view && route.ready?.(access.view));
}
/**
 * Code and authorized primary data start together, before the destination mounts, and the code
 * of what the sidebar of pages it is under draws, so that is in as it opens.
 */
export function prepareRoute(
  scope: Route['scope'],
  page: string,
  access?: Access,
  immediate = false,
) {
  const route = findRoute(scope, page);
  if (!route || (access && !allowed(route, access))) return Promise.resolve();
  if (access) prefetchRouteData(page, access.view, access.session, immediate);
  const drawing = pageSidebar(scope, page)?.draw;
  if (drawing) void loadSidebar(drawing).catch(() => undefined);
  return route.preload(access);
}
/**
 * The sidebar of pages `page` is under, while the person may open the page: the one the sidebar
 * shows as the page opens.
 */
export function sidebarOf(scope: Route['scope'], page: string, access: Access) {
  const route = findRoute(scope, page);
  return route && allowed(route, access) && available(route, access)
    ? pageSidebar(scope, page)
    : undefined;
}
/** The app's sidebar for `access`: the pages it lists, and its sidebars of pages with theirs. */
export function navigation(scope: Route['scope'], access: Access): NavItem[] {
  const item = (route: Route): NavItem => ({
    id: route.id,
    label: route.label,
    icon: route.icon,
    badge: route.badge,
    preload: () => prepareRoute(scope, route.id, access),
  });
  return sidebarItems(scope, access, table).map((each) =>
    'sidebar' in each
      ? {
          id: each.sidebar.id,
          label: each.sidebar.label,
          icon: each.sidebar.icon,
          // The first of its pages, with what the sidebar draws of its own.
          preload: () => prepareRoute(scope, each.pages[0]!.id, access),
          sidebar: each.sidebar,
          pages: each.pages.map(item),
        }
      : item(each),
  );
}

/**
 * The page a DSP opens on: the landing page a feature declares, or else the first one this
 * person may open.
 */
export const landing = (access: Access) =>
  landingPage() ??
  table.find((route): route is DspRoute => route.scope === 'dsp' && listed(route, access))?.id;

/** The page for an address, or the app's wording for one this person cannot open. */
function PageContent({
  page,
  reopen,
  ...context
}: PageContext & { page: string; view?: DspView; reopen: () => Promise<void> }) {
  const { session, view } = context;
  const route = findRoute(view ? 'dsp' : 'platform', page);
  const open = route && allowed(route, context) ? route : undefined;
  if (view) {
    if (open?.scope === 'dsp') {
      const page = { ...context, view, reopen };
      return (
        <OpenPage value={page}>
          {open.tabs ? (
            open.render(page)
          ) : (
            <PageWithTabs key={open.id} page={open.id} label={open.label}>
              {open.render(page)}
            </PageWithTabs>
          )}
        </OpenPage>
      );
    }
    // A DSP with no page this person may open, as one without features, has nothing to show.
    if (!page && !landing({ session, view }))
      return <Empty title="Nothing here yet">This DSP has no pages you can open.</Empty>;
    // A page of a feature the DSP lacks does not exist for it.
    const missing = route?.scope === 'dsp' && route.feature && !hasFeature(view, route.feature);
    return (
      <ErrorBox
        message={missing ? 'Page not found.' : 'This page is not available for your role.'}
      />
    );
  }
  if (open?.scope === 'platform') return open.render(context);
  // Members have one platform page: the DSPs they belong to.
  const dsps = findRoute('platform', 'dsps');
  if (!session.user.platformOwner && dsps?.scope === 'platform') return dsps.render(context);
  return <ErrorBox message="Page not found." />;
}

export function Page(props: Parameters<typeof PageContent>[0]) {
  const address = props.view
    ? dspHash(props.view.dsp.id, props.page as DspRouteId)
    : `#${props.page}`;
  return (
    <PageBoundary resetKey={props.page}>
      <Suspense fallback={<Loading />}>
        <NavigationStateContext value={address}>
          <PageContent {...props} />
        </NavigationStateContext>
      </Suspense>
    </PageBoundary>
  );
}
