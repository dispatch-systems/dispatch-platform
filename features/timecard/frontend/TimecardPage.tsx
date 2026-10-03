import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import {
  readUpdateState,
  useUpdateState,
} from '../../../core/shell/frontend/runtime/browser-update.js';
import {
  lazy,
  Suspense,
  useEffect,
  useLayoutEffect,
  useState,
  useTransition,
  type ReactNode,
} from 'react';
import { ArrowRight, RefreshCw, Settings } from 'lucide-react';
import type { Connection } from '../../../core/collection/api/index.js';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { EmployeeTimecardResponse } from '../api/index.js';
import { paycomDefaults } from './paycom.js';
import { api, useCachedData, useData } from '../../../core/shell/frontend/runtime/api.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { syncEmployeeTimecard, usePaycomSettings } from '../api/client.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { collectionData } from '../../../core/shell/frontend/runtime/data-policy.js';
import { useCollectionUpdates } from './live-collection.js';
import { ErrorBox, Header, Loading, Tabs } from '../../../core/shell/frontend/ui/index.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { randomId } from '../../../core/shell/frontend/lib/random-id.js';
import { timecardPeriod } from './timecard-format.js';
import { prefetchRouteData } from '../../../core/shell/frontend/runtime/route-prefetch.js';
import { prefetchTimecardTab } from './prefetch.js';
import { usePaycomDate } from './DateControls.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { dspHash, navigate } from '../../../core/shell/frontend/runtime/navigation.js';
import { SourceSyncStatus, type SyncSource } from './SourceSyncStatus.js';

const loadEmployees = () => import('./tabs/employees/EmployeesPage.js');
const loadTimecards = () => import('./tabs/daily/TimecardsPage.js');
const loadMeals = () => import('./tabs/meal-breaks/MealBreaksPage.js');
const loadSettings = () => import('./settings/index.js');
const EmployeesPage = lazy(() => loadEmployees().then((m) => ({ default: m.EmployeesPage })));
const TimecardsPage = lazy(() => loadTimecards().then((m) => ({ default: m.TimecardsPage })));
const MealBreaksPage = lazy(() => loadMeals().then((m) => ({ default: m.MealBreaksPage })));

function loadTab(tab: string) {
  return tab === 'employees'
    ? loadEmployees()
    : tab === 'meal-breaks'
      ? loadMeals()
      : loadTimecards();
}

// Each is a tab the platform switches on its own; the page has at least one while on.
const tabs = [
  ['timecards', 'Timecard', 'timecard.daily'],
  ['meal-breaks', 'Meal Breaks', 'timecard.meal_breaks'],
  ['employees', 'Employees', 'timecard.employees'],
] as const;

function selectedTimecardTab(view: DspView) {
  const selected = readUpdateState<string | undefined>(
    'paycom-tab',
    undefined,
    dspHash(view.dsp.id, 'paycom'),
  );
  const shown = tabs.filter(([, , feature]) => hasFeature(view, feature));
  return shown.some(([id]) => id === selected) ? selected! : (shown[0]?.[0] ?? 'timecards');
}

const committedTabs = new Set<string>();
function CommittedTab({ tab, children }: { tab: string; children: ReactNode }) {
  useLayoutEffect(() => {
    committedTabs.add(tab);
  }, [tab]);
  return children;
}

export function preloadTimecardPage(view: DspView) {
  return loadTab(selectedTimecardTab(view));
}

/** Import completion alone does not mean React has resolved the selected lazy component. */
export function isTimecardPageReady(view: DspView) {
  return committedTabs.has(selectedTimecardTab(view));
}

export function PaycomPage({ view }: { view: DspView }) {
  const canCollect = can(view, 'collections.run');
  const [selectedTab, setTab] = useUpdateState<string | undefined>('paycom-tab', undefined);
  const [pending, startTransition] = useTransition();
  const { date, today, selectDate } = usePaycomDate(view.dsp.id, view.dsp.timezone);
  const preferences = usePaycomSettings();
  useCollectionUpdates();
  const shownTabs = tabs.filter(([, , feature]) => hasFeature(view, feature));
  const tab = shownTabs.some(([id]) => id === selectedTab)
    ? selectedTab!
    : (shownTabs[0]?.[0] ?? 'timecards');
  const [syncRevision, setSyncRevision] = useState(0);
  const [collecting, setCollecting] = useState(false);
  const overview = useCachedData<{
    connection: Connection;
    workforce: { collectedAt: string | null };
  }>('/api/dsp/paycom/status', performancePolicy.recoveryPollMs);
  const syncState = useData<{
    date: string;
    scopeAvailable: boolean;
    paycom: SyncSource;
    flex: SyncSource;
  }>(
    `/api/dsp/jobs/meal-breaks?date=${date}`,
    collecting ? performancePolicy.activeCollectionPollMs : performancePolicy.recoveryPollMs,
    String(syncRevision),
    `${date}:${syncRevision}`,
    true,
  );
  // The last known state stays up while another date loads so the page does not shift.
  const sourceState = syncState.data ?? syncState.stale;
  const sourceCurrent = syncState.data?.date === date;
  const { error, refresh } = overview;
  const data = overview.data?.connection;
  const meals = tab === 'meal-breaks';
  const timecards = tab === 'timecards';
  const daily = timecards || meals;
  const activeSync = sourceState?.paycom.active || sourceState?.flex.active;
  useEffect(() => setCollecting(Boolean(activeSync)), [activeSync]);
  const collectedAt = overview.data?.workforce.collectedAt;
  useEffect(() => {
    if (collectedAt) dataCache.observeVersion('paycom', collectedAt, collectionData);
  }, [collectedAt]);
  const refreshKey = String(syncRevision);
  useEffect(() => {
    if (sourceState?.flex.collectedAt)
      dataCache.observeVersion('flex', sourceState.flex.collectedAt, (url) =>
        url.startsWith('/api/dsp/paycom/meal-breaks'),
      );
  }, [sourceState?.flex.collectedAt]);
  const syncUnavailable = daily
    ? !sourceState
      ? 'Checking connections…'
      : !sourceState.paycom.enabled
        ? 'Connect Paycom in Settings → Connections to sync.'
        : !sourceState.flex.enabled
          ? 'Connect Cortex in Settings → Connections to sync Flex.'
          : !sourceState.scopeAvailable
            ? 'Complete your DSP profile with a station code to sync Flex.'
            : ''
    : !data?.enabled
      ? 'Connect Paycom to sync.'
      : '';
  const canConnect = can(view, 'connections.manage');
  const sync = useAction(
    async (timecard?: EmployeeTimecardResponse) => {
      try {
        if (timecard) {
          await syncEmployeeTimecard(timecard.employee.code, timecard.period, randomId());
        } else if (daily)
          await api('/api/dsp/jobs/meal-breaks', {
            requestId: randomId(),
            date,
          });
      } finally {
        refresh();
        // Keep every Sync Now disabled until status read after this request arrives.
        setSyncRevision((value) => value + 1);
      }
    },
    {
      success: (timecard) =>
        timecard
          ? `${timecard.employee.name}’s timecard sync queued`
          : 'Flex and Paycom collections queued',
    },
  );
  const syncButton = (timecard?: EmployeeTimecardResponse) =>
    canCollect && (
      <button
        disabled={
          !!syncUnavailable ||
          !syncState.data ||
          syncState.validatedKey !== String(syncRevision) ||
          !!syncState.error ||
          sync.busy ||
          !!activeSync ||
          (daily && !sourceCurrent) ||
          (!daily && !timecard)
        }
        title={
          syncUnavailable ||
          (activeSync && 'A collection is in progress for this DSP.') ||
          (timecard
            ? `Sync ${timecard.employee.name} · ${timecardPeriod(timecard.period.from, timecard.period.to)}`
            : daily
              ? `Sync Flex and Paycom for ${date}`
              : 'Select an employee timecard to sync')
        }
        onClick={() => void sync.run(timecard)}
      >
        <RefreshCw size={16} />
        Sync now
      </button>
    );
  return (
    <div className={`paycom-page${daily ? ' paycom-daily-page' : ''}`}>
      <Header title="Timecard">
        {daily && canCollect && (
          <>
            <SourceSyncStatus
              name="Paycom"
              source={sourceState?.paycom}
              timezone={view.dsp.timezone}
              compact
            />
            <SourceSyncStatus
              name="Flex"
              source={sourceState?.flex}
              timezone={view.dsp.timezone}
              compact
            />
          </>
        )}
        {daily && syncButton()}
        {can(view, 'timecard.manage') && (
          <button
            onPointerEnter={() => {
              void loadSettings().catch(() => undefined);
              prefetchRouteData('paycom-settings', view);
            }}
            onFocus={() => {
              void loadSettings().catch(() => undefined);
              prefetchRouteData('paycom-settings', view);
            }}
            onClick={() => navigate(dspHash(view.dsp.id, 'paycom-settings'))}
          >
            {daily && <Settings size={16} />}
            Settings
          </button>
        )}
      </Header>
      {canConnect && <ErrorBox message={error} />}
      {canCollect && <ErrorBox message={syncState.error} />}
      <Tabs
        value={tab}
        onIntent={(value) => {
          void loadTab(value).catch(() => undefined);
          prefetchTimecardTab(value, date, view);
        }}
        onChange={(value) => {
          void loadTab(value).catch(() => undefined);
          prefetchTimecardTab(value, date, view, true);
          startTransition(() => setTab(value));
        }}
        items={shownTabs.map(([id, label]) => [id, label])}
        label="Timecard"
      />
      {canCollect && syncUnavailable && (
        <p className="paycom-sync-unavailable muted">{syncUnavailable}</p>
      )}
      <div aria-busy={pending || undefined} inert={pending}>
        <Suspense fallback={<Loading />}>
          {tab === 'meal-breaks' ? (
            <CommittedTab tab={tab}>
              <MealBreaksPage
                date={date}
                today={today}
                onDateChange={selectDate}
                refreshKey={refreshKey}
                timezone={view.dsp.timezone}
                dspId={view.dsp.id}
                canMatch={can(view, 'driver_match.manage')}
                preferences={preferences.data?.values ?? paycomDefaults}
              />
            </CommittedTab>
          ) : canConnect && data && !data.enabled && !overview.data?.workforce.collectedAt ? (
            <button
              className="primary connect-button"
              onClick={() => navigate(dspHash(view.dsp.id, 'settings', { tab: 'connections' }))}
            >
              Connect Paycom
              <ArrowRight size={16} />
            </button>
          ) : (
            <CommittedTab tab={tab}>
              <div className="embedded-page">
                {tab === 'employees' ? (
                  <EmployeesPage
                    refreshKey={`${refreshKey}:${syncRevision}`}
                    actions={(timecard) => (
                      <>
                        {canCollect && (
                          <SourceSyncStatus
                            name="Paycom"
                            source={
                              sourceState?.paycom && {
                                ...sourceState.paycom,
                                collectedAt: timecard?.collectedAt ?? null,
                                job: timecard?.syncStatus ? { status: timecard.syncStatus } : null,
                                active:
                                  !!timecard?.syncStatus &&
                                  ['queued', 'running', 'waiting_verification'].includes(
                                    timecard.syncStatus,
                                  ),
                                jobDate: timecard?.period.from ?? null,
                              }
                            }
                            timezone={view.dsp.timezone}
                            compact
                          />
                        )}
                        {canCollect && sourceState?.flex.active && (
                          <SourceSyncStatus
                            name="Flex"
                            source={sourceState.flex}
                            timezone={view.dsp.timezone}
                          />
                        )}
                        {syncButton(timecard)}
                      </>
                    )}
                  />
                ) : (
                  <TimecardsPage
                    date={date}
                    onDateChange={selectDate}
                    refreshKey={refreshKey}
                    timezone={view.dsp.timezone}
                    preferences={preferences.data?.values ?? paycomDefaults}
                  />
                )}
              </div>
            </CommittedTab>
          )}
        </Suspense>
      </div>
    </div>
  );
}
