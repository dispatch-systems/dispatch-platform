import { lazy, Suspense, type ReactNode } from 'react';
import {
  Bot,
  Building2,
  CalendarDays,
  FlaskConical,
  House,
  ScrollText,
  Settings,
  Shirt,
  ClipboardCheck,
  Users,
  type LucideIcon,
} from 'lucide-react';
import type { DspView, Feature, SessionView } from '../../../shared/contracts/index.js';
import { ErrorBox } from '../ui/ErrorBox.js';
import { Loading } from '../ui/Loading.js';
import { PageBoundary } from '../ui/PageBoundary.js';
import { hasFeature } from './features.js';
import { can } from './permissions.js';
import { routeMeta, type DspRouteId, type PlatformRouteId, type RouteMeta } from './route-meta.js';
import { NavigationStateContext } from './browser-update.js';
import { isTimecardDataReady, prefetchRouteData } from './route-prefetch.js';

const loadAgents = () => import('../features/agents/index.js');
const AgentsPage = lazy(() => loadAgents().then((module) => ({ default: module.AgentsPage })));
const AuthorizePage = lazy(() =>
  loadAgents().then((module) => ({ default: module.AuthorizePage })),
);
const loadAudit = () => import('../features/audit/index.js');
const AuditPage = lazy(() => loadAudit().then((module) => ({ default: module.AuditPage })));
const loadHome = () => import('../features/home/index.js');
const HomePage = lazy(() => loadHome().then((module) => ({ default: module.HomePage })));
const loadPlatform = () => import('../features/platform/index.js');
const loadDiagnostics = () => import('../features/platform/diagnostics/index.js');
const loadPicker = () => import('../features/platform/picker.js');
const DiagnosticsPage = lazy(() =>
  loadDiagnostics().then((module) => ({ default: module.DiagnosticsPage })),
);
const DspsPage = lazy(() => loadPlatform().then((module) => ({ default: module.DspsPage })));
const DspPicker = lazy(() => loadPicker().then((module) => ({ default: module.DspPicker })));
const loadSettings = (access?: Access) =>
  import('../features/settings/index.js').then(async (module) => {
    await module.preloadSettingsPage(access?.view);
    return module;
  });
const SettingsPage = lazy(() =>
  loadSettings().then((module) => ({ default: module.SettingsPage })),
);
const loadTeam = () => import('../features/team/index.js');
const TeamPage = lazy(() => loadTeam().then((module) => ({ default: module.TeamPage })));
const loadUniforms = () => import('../features/uniforms/index.js');
const UniformInventoryPage = lazy(() =>
  loadUniforms().then((module) => ({ default: module.UniformInventoryPage })),
);
let timecardReady: ((view: DspView) => boolean) | undefined;
const loadTimecard = (access?: Access) =>
  import('../features/timecard/index.js').then(async (module) => {
    timecardReady = module.isTimecardPageReady;
    if (access?.view) await module.preloadTimecardPage(access.view);
    return module;
  });
const loadTimecardSettings = () => import('../features/timecard/settings/index.js');
let dvicReady: ((view: DspView) => boolean) | undefined;
const loadDvic = () =>
  import('../features/dvic/index.js').then((module) => {
    dvicReady = module.isDvicPageReady;
    return module;
  });
const DvicPage = lazy(() => loadDvic().then((module) => ({ default: module.DvicPage })));
const PaycomPage = lazy(() => loadTimecard().then((module) => ({ default: module.PaycomPage })));
const PaycomSettingsPage = lazy(() =>
  loadTimecardSettings().then((module) => ({ default: module.PaycomSettingsPage })),
);

type Access = { session: SessionView; view?: DspView };
type PageContext = { session: SessionView };
type DspPageContext = PageContext & { view: DspView; reopen: () => Promise<void> };
type Entry<Context> = {
  icon?: LucideIcon;
  preload: (access?: Access) => Promise<unknown>;
  /** Whether the sidebar lists the page. */
  nav: boolean | ((access: Access) => boolean);
  /** Who may open the page; omitted means everyone in the scope. */
  permission?: (access: Access) => boolean;
  /** The feature the page belongs to; a DSP without it has no such page. */
  feature?: Feature;
  render: (context: Context) => ReactNode;
};
type Route =
  | (RouteMeta & { scope: 'dsp' } & Entry<DspPageContext>)
  | (RouteMeta & { scope: 'platform' } & Entry<PageContext>);

const platformOwner = ({ session }: Access) => session.user.platformOwner;

// Every page declared in route-meta.ts gets its navigation, access and component here.
const dspPages: Record<DspRouteId, Entry<DspPageContext>> = {
  dvic: {
    preload: loadDvic,
    icon: ClipboardCheck,
    nav: true,
    feature: 'dvic',
    permission: ({ view }) => can(view, 'dvic.view'),
    render: ({ view }) => <DvicPage key={view.token} view={view} />,
  },
  uniforms: {
    preload: loadUniforms,
    icon: Shirt,
    nav: true,
    feature: 'uniforms',
    permission: ({ view }) => can(view, 'uniforms.view'),
    render: ({ view }) => <UniformInventoryPage key={view.token} view={view} />,
  },
  overview: {
    preload: loadHome,
    icon: House,
    nav: true,
    render: () => <HomePage />,
  },
  paycom: {
    preload: loadTimecard,
    icon: CalendarDays,
    nav: true,
    feature: 'timecard',
    // The link stays put while a view loads; the page itself waits for the view.
    permission: ({ view }) => !view || can(view, 'timecard.view'),
    render: ({ view }) => <PaycomPage view={view} />,
  },
  'paycom-settings': {
    preload: loadTimecardSettings,
    nav: false,
    feature: 'timecard',
    permission: ({ view }) => can(view, 'timecard.manage'),
    render: ({ view }) => <PaycomSettingsPage dspId={view.dsp.id} />,
  },
  team: {
    preload: loadTeam,
    icon: Users,
    nav: true,
    permission: ({ view }) =>
      can(view, 'members.invite') || can(view, 'members.manage') || can(view, 'roles.manage'),
    render: ({ view, reopen }) => <TeamPage view={view} reopen={reopen} />,
  },
  settings: {
    preload: loadSettings,
    icon: Settings,
    nav: true,
    render: ({ session, view }) => <SettingsPage session={session} view={view} />,
  },
};
const platformPages: Record<PlatformRouteId, Entry<PageContext>> = {
  dsps: {
    preload: (access) => (access?.session.user.platformOwner ? loadPlatform() : loadPicker()),
    icon: Building2,
    nav: true,
    render: ({ session }) =>
      session.user.platformOwner ? <DspsPage /> : <DspPicker session={session} />,
  },
  jobs: {
    preload: loadDiagnostics,
    icon: FlaskConical,
    nav: true,
    permission: platformOwner,
    render: () => <DiagnosticsPage />,
  },
  agents: {
    preload: loadAgents,
    icon: Bot,
    nav: true,
    permission: platformOwner,
    render: () => <AgentsPage />,
  },
  authorize: {
    preload: loadAgents,
    nav: false,
    permission: platformOwner,
    render: () => <AuthorizePage />,
  },
  audit: {
    preload: loadAudit,
    icon: ScrollText,
    nav: true,
    permission: platformOwner,
    render: () => <AuditPage />,
  },
  account: {
    preload: loadSettings,
    icon: Settings,
    nav: platformOwner,
    render: ({ session }) => <SettingsPage session={session} />,
  },
};

const table: readonly Route[] = routeMeta.map((meta) =>
  meta.scope === 'dsp' ? { ...meta, ...dspPages[meta.id] } : { ...meta, ...platformPages[meta.id] },
);
const allowed = (route: Route, access: Access) => !route.permission || route.permission(access);

export const findRoute = (scope: Route['scope'], page: string) =>
  table.find((route) => route.scope === scope && route.id === page);
/** A previously rendered tab with admitted cached data can commit before the next paint. */
export function canNavigateImmediately(scope: Route['scope'], page: string, access: Access) {
  return Boolean(
    scope === 'dsp' &&
    access.view &&
    ((page === 'paycom' && timecardReady?.(access.view) && isTimecardDataReady(access.view)) ||
      (page === 'dvic' && dvicReady?.(access.view))),
  );
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
  return session.user.platformOwner ? (
    <ErrorBox message="Page not found." />
  ) : (
    <DspPicker session={session} />
  );
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
