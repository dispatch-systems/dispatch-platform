import { Suspense } from 'react';
import type { DspView } from '../../shared/contracts/index.js';
import { ErrorBox } from '../../core/shell/frontend/ui/ErrorBox.js';
import { Loading } from '../../core/shell/frontend/ui/Loading.js';
import { PageBoundary } from '../../core/shell/frontend/ui/PageBoundary.js';
import { hasFeature } from '../../core/shell/frontend/runtime/features.js';
import { NavigationStateContext } from '../../core/shell/frontend/runtime/browser-update.js';
import { prefetchRouteData } from '../../core/shell/frontend/runtime/route-prefetch.js';
import type { Access, PageContext, Route } from '../../core/shell/frontend/runtime/slots.js';
import { features } from './features.js';

// Every owner's pages, each with its navigation, access and component, in sidebar order.
const table: readonly Route[] = features.flatMap((feature) => feature.routes ?? []);
const allowed = (route: Route, access: Access) => !route.permission || route.permission(access);

export const findRoute = (scope: Route['scope'], page: string) =>
  table.find((route) => route.scope === scope && route.id === page);
/** A previously rendered tab with admitted cached data can commit before the next paint. */
export function canNavigateImmediately(scope: Route['scope'], page: string, access: Access) {
  const route = findRoute(scope, page);
  return Boolean(route?.scope === 'dsp' && access.view && route.ready?.(access.view));
}
/** Code and authorized primary data start together, before the destination mounts. */
export function prepareRoute(
  scope: Route['scope'],
  page: string,
  access?: Access,
  immediate = false,
) {
  const route = findRoute(scope, page);
  if (!route || (access && !allowed(route, access))) return Promise.resolve();
  if (access) prefetchRouteData(page, access.view, access.session, immediate);
  return route.preload(access);
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
    if (open?.scope === 'dsp') return open.render({ ...context, view, reopen });
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
  const address = props.view ? `#dsp/${props.view.dsp.id}/${props.page}` : `#${props.page}`;
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
