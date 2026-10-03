import { lazy, Suspense, useState, useTransition, type ReactNode } from 'react';
import type { DspView, SessionView } from '../../../shared/contracts/index.js';
import { Header, Loading, Tabs } from '../../../core/shell/frontend/ui/index.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { connectionFeatures } from '../../../core/shell/frontend/runtime/features.js';
import { DriverMatchTabLabel } from '../../driver_match/frontend/badge.js';
import { hashQuery, replaceHashQuery } from '../../../core/shell/frontend/runtime/navigation.js';
import { ProfileBadge } from '../../../core/accounts/frontend/settings/ProfileBadge.js';
import { prefetchSettingsTab } from '../../../core/shell/frontend/runtime/route-prefetch.js';

const loadConnections = () => import('../../../core/collection/frontend/index.js');
const loadDriverMatch = () => import('../../driver_match/frontend/index.js');
const loadSecurity = () => import('../../../core/accounts/frontend/settings/SecuritySettings.js');
const loadRouteData = () => import('../../routes/frontend/settings/RouteDataSettings.js');
const loadTheme = () => import('../../../core/accounts/frontend/settings/ThemeSection.js');
const ConnectionsPage = lazy(() => loadConnections().then((m) => ({ default: m.ConnectionsPage })));
const DriverMatchSettings = lazy(() =>
  loadDriverMatch().then((m) => ({ default: m.DriverMatchSettings })),
);
const SecuritySettings = lazy(() => loadSecurity().then((m) => ({ default: m.SecuritySettings })));
const RouteDataSettings = lazy(() =>
  loadRouteData().then((m) => ({ default: m.RouteDataSettings })),
);
const ThemeSection = lazy(() => loadTheme().then((m) => ({ default: m.ThemeSection })));

const panels: Record<string, () => Promise<unknown>> = {
  connections: loadConnections,
  'driver-match': loadDriverMatch,
  security: loadSecurity,
  data: loadRouteData,
  theme: loadTheme,
};
const loadTab = (tab: string) => panels[tab]?.() ?? Promise.resolve();

export function preloadSettingsPage(view?: DspView) {
  const tab = hashQuery().get('tab') || 'general';
  if (
    (tab === 'connections' && !can(view, 'connections.manage')) ||
    (tab === 'driver-match' && !can(view, 'driver_match.manage')) ||
    (tab === 'data' && !can(view, 'routes.manage'))
  )
    return Promise.resolve();
  return loadTab(tab);
}

export function SettingsPage({ session, view }: { session: SessionView; view?: DspView }) {
  const [requestedTab, setTab] = useState(hashQuery().get('tab') || 'general');
  const [pending, startTransition] = useTransition();
  const connections = can(view, 'connections.manage');
  const routeData = can(view, 'routes.manage');
  const driverMatch = can(view, 'driver_match.manage');
  const tabs: (readonly [string, ReactNode])[] = [
    // The id stays `general` so existing links to the tab keep working.
    ['general', 'Profile'],
    ['security', 'Security'],
    ...(connections ? [['connections', 'Connections'] as const] : []),
    ...(driverMatch
      ? [
          [
            'driver-match',
            <DriverMatchTabLabel key="label" active={requestedTab === 'driver-match'} />,
          ] as const,
        ]
      : []),
    ...(routeData ? [['data', 'Data'] as const] : []),
    ['theme', 'Theme'],
  ];
  const tab = tabs.some(([id]) => id === requestedTab) ? requestedTab : 'general';
  return (
    <>
      <Header title="Settings" />
      <Tabs
        value={tab}
        onIntent={(value) => {
          void loadTab(value).catch(() => undefined);
          prefetchSettingsTab(value, view, session);
        }}
        onChange={(value) => {
          void loadTab(value).catch(() => undefined);
          prefetchSettingsTab(value, view, session, true);
          replaceHashQuery({ tab: value });
          startTransition(() => setTab(value));
        }}
        items={tabs}
        label="Settings"
      />
      <div aria-busy={pending || undefined} inert={pending}>
        <Suspense fallback={<Loading />}>
          {tab === 'general' && <ProfileBadge session={session} view={view} />}
          {tab === 'security' && <SecuritySettings />}
          {tab === 'connections' && connections && view && (
            <div className="settings-connections">
              <ConnectionsPage
                development={session.providerMode === 'fixture'}
                timezone={view.dsp.timezone}
                providers={connectionFeatures(view.features).map((f) => f.id)}
              />
            </div>
          )}
          {tab === 'driver-match' && driverMatch && view && (
            <DriverMatchSettings timezone={view.dsp.timezone} />
          )}
          {tab === 'data' && routeData && view && (
            <RouteDataSettings timeZone={view.dsp.timezone} />
          )}
          {tab === 'theme' && <ThemeSection userId={session.user.id} />}
        </Suspense>
      </div>
    </>
  );
}
