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
  loadSidebar,
  sidebarOwner,
  type Access,
  type DspRouteId,
  type PageContext,
  type Route,
} from '../../core/shell/frontend/runtime/slots.js';
import { dspHash } from '../../core/shell/frontend/runtime/navigation.js';
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
const present = (route: Route, access: Access) =>
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
 * of the sidebar it draws, so that is in as it opens.
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
  const sidebar = sidebarOwner(scope, page)?.sidebar;
  if (sidebar) void loadSidebar(sidebar).catch(() => undefined);
  return route.preload(access);
}
/**
 * The sidebar drawn in place of the app's while `page` is open: its own, or that of the page it
 * is under, while the person may open that page.
 */
export function sidebarOf(scope: Route['scope'], page: string, access: Access) {
  const owner = sidebarOwner(scope, page);
  return owner && allowed(owner, access) && present(owner, access) ? owner.sidebar : undefined;
}
export const navigation = (scope: Route['scope'], access: Access) =>
  table
    .filter(
      (route) =>
        route.scope === scope &&
        (typeof route.nav === 'function' ? route.nav(access) : route.nav) &&
        allowed(route, access),
    )
    .map((route) => ({ ...route, preload: () => prepareRoute(scope, route.id, access) }));

/**
 * The page a DSP opens on: the landing page a feature declares, or else the first one this
 * person may open.
 */
export const landing = (access: Access) =>
  landingPage() ?? navigation('dsp', access).find((route) => route.scope === 'dsp')?.id;

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
