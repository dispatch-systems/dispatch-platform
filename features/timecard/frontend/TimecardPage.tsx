import {
  readUpdateState,
  useUpdateState,
} from '../../../core/shell/frontend/runtime/navigation-state.js';
import {
  lazy,
  Suspense,
  use,
  useLayoutEffect,
  useState,
  useTransition,
  type ReactNode,
} from 'react';
import { ArrowRight, RefreshCw, Settings } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { EmployeeTimecardPeriod } from '../api/index.js';
import { paycomDefaults } from './paycom.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { usePaycomSettings } from '../api/client.js';
import { useCollectionUpdates } from '../../../core/collection/frontend/live-collection.js';
import { ErrorBox, Header, Loading, Tabs } from '../../../core/shell/frontend/ui/index.js';
import { HostedTab, OpenPage, usePageTabs } from '../../../core/shell/frontend/runtime/PageTabs.js';
import { settingsPage } from '../../../core/shell/frontend/runtime/slots.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { prefetchRouteData } from '../../../core/shell/frontend/runtime/route-prefetch.js';
import { prefetchTimecardTab } from './prefetch.js';
import { usePaycomDate } from './DateControls.js';
import {
  dspHash,
  navigate,
  pageHash,
  settingsHash,
} from '../../../core/shell/frontend/runtime/navigation.js';
import { SourceSyncStatus } from './SourceSyncStatus.js';
import { EmployeePeriodSync } from './tabs/employees/EmployeePeriodSync.js';
import { useDailySync } from './useDailySync.js';

const loadEmployees = () => import('./tabs/employees/EmployeesPage.js');
const loadTimecards = () => import('./tabs/daily/TimecardsPage.js');
const loadMeals = () => import('./tabs/meal-breaks/MealBreaksPage.js');
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
  const [employeeSelection, setEmployeeSelection] = useUpdateState<
    { code: string; period: EmployeeTimecardPeriod | null; date?: string } | undefined
  >('employee-selection', undefined);
  const [employeeView, setEmployeeView] = useUpdateState<'timecard' | 'meals'>(
    'employee-view',
    'timecard',
  );
  const [pending, startTransition] = useTransition();
  const { date, today, selectDate } = usePaycomDate(view.dsp.id, view.dsp.timezone);
  const settings = usePaycomSettings();
  useCollectionUpdates();
  const shownTabs = tabs.filter(([, , feature]) => hasFeature(view, feature));
  const ownTab = shownTabs.some(([id]) => id === selectedTab)
    ? selectedTab!
    : (shownTabs[0]?.[0] ?? 'timecards');
  const hosting = usePageTabs(
    'paycom',
    shownTabs.map(([id, label]) => [id, label]),
  );
  const tab = hosting.open?.id ?? ownTab;
  const meals = tab === 'meal-breaks';
  const timecards = tab === 'timecards';
  const daily = timecards || meals;
  const employees = tab === 'employees';
  // Where the Employees tab draws its sync, in the heading as the daily tabs draw theirs.
  const [employeeSync, setEmployeeSync] = useState<HTMLDivElement | null>(null);
  const { overview, sourceState, activeSync, syncUnavailable, refreshKey, sync, onQueued } =
    useDailySync(date, daily);
  const { error } = overview;
  const data = overview.data?.connection;
  const canConnect = can(view, 'connections.manage');
  const connections = settingsHash(view.dsp.id, 'connections');
  // Its own page of Settings, for those who manage it.
  const ownSettings = can(view, 'timecard.manage')
    ? settingsHash(view.dsp.id, 'timecard', view)
    : undefined;
  const warmSettings = () => {
    const host = settingsPage();
    if (host) prefetchRouteData(host, view);
  };
  const page = use(OpenPage);
  const syncButton = () =>
    canCollect && (
      <button
        disabled={sync.disabled}
        title={
          syncUnavailable ||
          (activeSync && 'A collection is in progress for this DSP.') ||
          `Sync Flex and Paycom for ${date}`
        }
        onClick={() => void sync.run()}
      >
        <RefreshCw size={16} />
        Sync now
      </button>
    );
  return (
    <div className={`paycom-page${daily || employees ? ' paycom-sync-page' : ''}`}>
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
        {employees && canCollect && <div ref={setEmployeeSync} className="paycom-employee-sync" />}
        {ownSettings && (
          <button
            onPointerEnter={warmSettings}
            onFocus={warmSettings}
            onClick={() => navigate(ownSettings)}
          >
            {daily && <Settings size={16} />}
            Settings
          </button>
        )}
      </Header>
      {canConnect && <ErrorBox message={error} />}
      {daily && canCollect && <ErrorBox message={sync.error} />}
      <Tabs
        value={tab}
        onIntent={(value) => {
          if (!shownTabs.some(([id]) => id === value)) return hosting.warm(value);
          void loadTab(value).catch(() => undefined);
          prefetchTimecardTab(value, date, view);
        }}
        onChange={(value) =>
          hosting.change(value, (own) => {
            void loadTab(own).catch(() => undefined);
            prefetchTimecardTab(own, date, view, true);
            startTransition(() => setTab(own));
          })
        }
        items={hosting.items}
        label="Timecard"
      />
      {canCollect && syncUnavailable && (
        <p className="paycom-sync-unavailable muted">{syncUnavailable}</p>
      )}
      <div className="paycom-tab" aria-busy={pending || undefined} inert={pending}>
        <Suspense fallback={<Loading />}>
          {hosting.open ? (
            <HostedTab tab={hosting.open} />
          ) : tab === 'meal-breaks' ? (
            <CommittedTab tab={tab}>
              <MealBreaksPage
                date={date}
                today={today}
                onDateChange={selectDate}
                refreshKey={refreshKey}
                timezone={view.dsp.timezone}
                onOpenEmployee={
                  hasFeature(view, 'timecard.employees')
                    ? (code, date) => {
                        setEmployeeSelection({ code, period: null, date });
                        setEmployeeView('meals');
                        startTransition(() => setTab('employees'));
                      }
                    : undefined
                }
                driverMatch={page && pageHash(page, 'driver-match')}
                settings={settings.data ?? paycomDefaults}
              />
            </CommittedTab>
          ) : canConnect &&
            connections &&
            data &&
            !data.enabled &&
            !overview.data?.workforce.collectedAt ? (
            <button className="primary connect-button" onClick={() => navigate(connections)}>
              Connect Paycom
              <ArrowRight size={16} />
            </button>
          ) : (
            <CommittedTab tab={tab}>
              <div className="embedded-page">
                {employees ? (
                  <EmployeesPage
                    selection={employeeSelection}
                    onSelectionChange={setEmployeeSelection}
                    employeeView={
                      hasFeature(view, 'timecard.meal_breaks') ? employeeView : 'timecard'
                    }
                    onViewChange={setEmployeeView}
                    showMeals={hasFeature(view, 'timecard.meal_breaks')}
                    today={today}
                    refreshKey={`${refreshKey}:${refreshKey}`}
                    actionsTarget={employeeSync}
                    actions={(timecard) => (
                      <EmployeePeriodSync
                        key={view.dsp.id}
                        period={employeeSelection?.period ?? timecard?.period}
                        refreshKey={refreshKey}
                        onQueued={onQueued}
                      />
                    )}
                  />
                ) : (
                  <TimecardsPage
                    date={date}
                    onDateChange={selectDate}
                    refreshKey={refreshKey}
                    timezone={view.dsp.timezone}
                    settings={settings.data ?? paycomDefaults}
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
