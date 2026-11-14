import { useSettled } from '../../../core/shell/frontend/runtime/useSettled.js';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { Job } from '../../../core/collection/api/index.js';
import { useCollectionUpdates } from '../../../core/collection/frontend/live-collection.js';
import type { DvicStatus } from '../api/index.js';
import { ApiError, view as admittedToken } from '../../../core/shell/frontend/runtime/api.js';
import {
  cancelDvicJob,
  collectDvic,
  useDvicStatus,
  summaryUrl,
  useSummary,
} from '../api/client.js';
import {
  readUpdateState,
  useUpdateState,
} from '../../../core/shell/frontend/runtime/navigation-state.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import {
  dspHash,
  hashQuery,
  replaceHashQuery,
  settingsHash,
  useHashQuery,
} from '../../../core/shell/frontend/runtime/navigation.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import {
  isCollected,
  latestCollected,
  publication as publishedAt,
  weekDays,
  weekStart,
  type VehicleClass,
} from './dvic.js';
import { localDate, shiftDate } from '../../../core/shell/frontend/lib/day.js';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { randomId } from '../../../core/shell/frontend/lib/random-id.js';
import { usePageTabs } from '../../../core/shell/frontend/runtime/PageTabs.js';

const activeJob = (job: Job) => ['queued', 'running', 'waiting_verification'].includes(job.status);
// Each view is a tab the platform switches on its own; the page has at least one while on.
const views = [
  ['day', 'Day', 'dvic.day'],
  ['week', 'Week', 'dvic.week'],
] as const;

/** The day a link opens the page on (`?day=`), as Home's Latest DVIC does: on the Day tab, unfiltered. */
const linkedDay = (query: URLSearchParams) => {
  const day = query.get('day');
  return day && /^\d{4}-\d{2}-\d{2}$/.test(day) ? day : undefined;
};

function selectedWeek(status: DvicStatus | undefined, today: string, chosenWeek?: string) {
  const latestDate = latestCollected(status, today);
  const start =
    chosenWeek && chosenWeek >= '2000-01-02' && chosenWeek <= today
      ? chosenWeek
      : weekStart(latestDate ?? today);
  return { start, latestDate };
}

let committed = false;
/** A committed module and a complete selected week can render before the next paint. */
export function isDvicPageReady(view: DspView) {
  if (
    !committed ||
    admittedToken !== view.token ||
    !can(view, 'dvic.view') ||
    !views.some(([, , feature]) => hasFeature(view, feature))
  )
    return false;
  const status = dataCache.peek('/api/dsp/dvic/status').data as DvicStatus | undefined;
  if (status === undefined) return false;
  const hash = dspHash(view.dsp.id, 'dvic');
  const linked = linkedDay(hashQuery());
  const chosenWeek = linked
    ? weekStart(linked)
    : readUpdateState<string | undefined>('dvic-week', undefined, hash);
  const { start } = selectedWeek(status, localDate(view.dsp.timezone), chosenWeek);
  const query = linked ? '' : readUpdateState('dvic-query', '', hash).trim().toLowerCase();
  const vehicles = linked ? '' : readUpdateState<VehicleClass>('dvic-vehicles', '', hash);
  const page = linked ? 0 : readUpdateState('dvic-week-page', 0, hash);
  return (
    dataCache.peek(summaryUrl(start, shiftDate(start, 6), query, vehicles, page)).data !== undefined
  );
}

export function useDvicPage(view: DspView) {
  useLayoutEffect(() => {
    committed = true;
  }, []);
  // A DVIC run is announced as it ends, wherever it started; while one runs, its status is
  // still read every few seconds, since its progress isn't announced.
  useCollectionUpdates();
  const [collecting, setCollecting] = useState(false);
  const status = useDvicStatus(
    collecting ? performancePolicy.activeCollectionPollMs : performancePolicy.recoveryPollMs,
  );
  const shownViews = views.filter(([, , feature]) => hasFeature(view, feature));
  // A link to a day shows it at once, then becomes the page's own choice, as if picked here.
  const [linked, setLinked] = useHashQuery(linkedDay);
  const [keptTab, setTab] = useUpdateState<'day' | 'week'>('dvic-tab', 'day');
  const chosenTab = linked ? 'day' : keptTab;
  const tab = shownViews.some(([id]) => id === chosenTab)
    ? chosenTab
    : (shownViews[0]?.[0] ?? chosenTab);
  const tabs = usePageTabs(
    'dvic',
    shownViews.map(([id, label]) => [id, label]),
  );
  const [keptWeek, setChosenWeek] = useUpdateState<string | undefined>('dvic-week', undefined);
  const chosenWeek = linked ? weekStart(linked) : keptWeek;
  const [keptDay, setChosenDay] = useUpdateState<string | undefined>('dvic-day', undefined);
  const chosenDay = linked ?? keptDay;
  const [keptQuery, setQuery] = useUpdateState('dvic-query', '');
  const query = linked ? '' : keptQuery;
  const search = useSettled(query, performancePolicy.employeeSearchDelayMs).trim().toLowerCase();
  const [keptVehicles, setVehicles] = useUpdateState<VehicleClass>('dvic-vehicles', '');
  const vehicles = linked ? '' : keptVehicles;
  const [keptPage, setPage] = useUpdateState('dvic-week-page', 0);
  const page = linked ? 0 : keptPage;
  const [selected, setSelected] = useState<string>();
  useEffect(() => {
    if (!linked) return;
    setTab('day');
    setChosenWeek(weekStart(linked));
    setChosenDay(linked);
    setQuery('');
    setVehicles('');
    setPage(0);
    setSelected(undefined);
    replaceHashQuery({});
    setLinked(undefined);
  }, [linked, setLinked, setTab, setChosenWeek, setChosenDay, setQuery, setVehicles, setPage]);
  const [pending, setPending] = useState<Job>();
  // A retry keeps its request's id, so a lost reply can't queue it twice; another week is
  // another request.
  const request = useRef<{ id: string; viewing: string } | null>(null);
  const today = localDate(view.dsp.timezone);
  const { start, latestDate } = selectedWeek(status.data, today, chosenWeek);
  const days = useMemo(() => weekDays(start), [start]);
  const end = days[6]!;
  // The latest collected day in this week, else the last day that has happened.
  const day =
    chosenDay && chosenDay >= start && chosenDay <= end && chosenDay <= today
      ? chosenDay
      : latestDate && latestDate >= start && latestDate <= end
        ? latestDate
        : ([...days].reverse().find((date) => date <= today) ?? start);
  const jobs = status.data?.jobs ?? [];
  const pendingStatus = jobs.find((job) => job.id === pending?.id) ?? pending;
  const running =
    jobs.find(activeJob) ?? (pendingStatus && activeJob(pendingStatus) ? pendingStatus : undefined);
  const { completed, checked, publication } = publishedAt(status.data);
  useEffect(() => {
    if (status.data)
      dataCache.observeVersion('dvic', publication, (key) =>
        ['summary?', 'day?', 'inspection?'].some((part) => key.startsWith('/api/dsp/dvic/' + part)),
      );
  }, [publication, status.data]);
  const inspections = useSummary(
    status.data ? summaryUrl(start, end, search, vehicles, page) : '',
    publication,
  );
  const summary = inspections.data;
  const daySummary = summary?.days.find((item) => item.date === day);
  const counts = new Map(summary?.days.map((item) => [item.date, item.count]));
  const filtered = !!(query || vehicles);
  const collected = (date: string) => isCollected(status.data?.covered ?? [], date);
  const shown = tab === 'day' ? (daySummary?.count ?? 0) : (summary?.totalInspections ?? 0);
  const newest = jobs[0];
  const canCollect = can(view, 'dvic.collect');
  const settings = can(view, 'dvic.manage')
    ? settingsHash(view.dsp.id, 'collectors', view, { collection: 'dvic' })
    : undefined;
  const active = Boolean(running);
  useEffect(() => setCollecting(active), [active]);
  const sync = useAction(
    async () => {
      if (request.current?.viewing !== start) request.current = { id: randomId(), viewing: start };
      const job: Job = await collectDvic(request.current.id, start).catch((cause) => {
        if (cause instanceof ApiError && cause.code === 'connection_required')
          throw new Error('Connect Cortex in Settings → Connections before syncing DVIC.');
        throw cause;
      });
      request.current = null;
      setPending(job);
      status.refresh();
    },
    { inline: true },
  );
  const cancel = useAction(
    async () => {
      if (!running) return;
      await cancelDvicJob(running.id);
      status.refresh();
    },
    { inline: true },
  );
  const selectWeek = (date: string, chosen?: string) => {
    if (date < '2000-01-02' || date > today) return;
    setChosenWeek(weekStart(date));
    setChosenDay(chosen);
    setSelected(undefined);
    setPage(0);
  };
  const selectDay = (date: string) => {
    setChosenDay(date);
    setSelected(undefined);
  };
  const changeQuery = (value: string) => {
    setQuery(value);
    setPage(0);
  };
  const changeVehicles = (value: VehicleClass) => {
    setVehicles(value);
    setPage(0);
  };
  return {
    status,
    tab,
    tabs,
    setTab,
    query,
    search,
    vehicles,
    page,
    setPage,
    selected,
    setSelected,
    today,
    start,
    latestDate,
    days,
    end,
    day,
    running,
    completed,
    checked,
    publication,
    inspections,
    summary,
    daySummary,
    counts,
    filtered,
    collected,
    shown,
    newest,
    canCollect,
    settings,
    sync,
    cancel,
    selectWeek,
    selectDay,
    changeQuery,
    changeVehicles,
  };
}
