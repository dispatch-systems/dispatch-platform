import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, RefreshCw, Settings2 } from 'lucide-react';
import type { DspView, Job } from '../../../../shared/contracts/index.js';
import type { DvicStatus } from '../../../../shared/contracts/dvic.js';
import { api, ApiError, useCachedData, view as admittedToken } from '../../app/api.js';
import { readUpdateState, useUpdateState } from '../../app/browser-update.js';
import { dataCache } from '../../app/data-cache.js';
import { hasFeature } from '../../app/features.js';
import { can } from '../../app/permissions.js';
import { dspHash } from '../../app/navigation.js';
import { useAction } from '../../app/useAction.js';
import {
  filterInspections,
  inspectionDate,
  inspectionWeekday,
  weekDays,
  weekStart,
  type VehicleClass,
} from '../../lib/dvic.js';
import { localDate, shiftDate } from '../../lib/meal-breaks.js';
import { performancePolicy } from '../../lib/performance-policy.js';
import { randomId } from '../../lib/random-id.js';
import { time } from '../../lib/format.js';
import { ErrorBox, Header, Loading, SearchInput, Tabs } from '../../ui/index.js';
import { CollectionSettings } from './CollectionSettings.js';
import { DayDigest } from './DayDigest.js';
import { InspectionDetail } from './InspectionDetail.js';
import { WeekGrid } from './WeekGrid.js';
import { inspectionWeekCacheKey, useInspections } from './useInspections.js';

const activeJob = (job: Job) => ['queued', 'running', 'waiting_verification'].includes(job.status);
// Each view is a tab the platform switches on its own; the page has at least one while on.
const views = [
  ['day', 'Day', 'dvic.day'],
  ['week', 'Week', 'dvic.week'],
] as const;

function selectedWeek(status: DvicStatus | undefined, today: string, chosenWeek?: string) {
  const latestDate = status?.reports
    .map((report) => report.maxDate)
    .filter((date): date is string => !!date && date <= today)
    .sort()
    .at(-1);
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
  const chosenWeek = readUpdateState<string | undefined>(
    'dvic-week',
    undefined,
    dspHash(view.dsp.id, 'dvic'),
  );
  const { start } = selectedWeek(status, localDate(view.dsp.timezone), chosenWeek);
  return dataCache.peek(inspectionWeekCacheKey(start, shiftDate(start, 6))).data !== undefined;
}

export function DvicPage({ view }: { view: DspView }) {
  useLayoutEffect(() => {
    committed = true;
  }, []);
  const [collecting, setCollecting] = useState(false);
  const status = useCachedData<DvicStatus>(
    '/api/dsp/dvic/status',
    collecting ? performancePolicy.activeCollectionPollMs : performancePolicy.recoveryPollMs,
  );
  const shownViews = views.filter(([, , feature]) => hasFeature(view, feature));
  const [chosenTab, setTab] = useUpdateState<'day' | 'week'>('dvic-tab', 'day');
  const tab = shownViews.some(([id]) => id === chosenTab)
    ? chosenTab
    : (shownViews[0]?.[0] ?? chosenTab);
  const [chosenWeek, setChosenWeek] = useUpdateState<string | undefined>('dvic-week', undefined);
  const [chosenDay, setChosenDay] = useUpdateState<string | undefined>('dvic-day', undefined);
  const [query, setQuery] = useUpdateState('dvic-query', '');
  const [vehicles, setVehicles] = useUpdateState<VehicleClass>('dvic-vehicles', '');
  const [page, setPage] = useUpdateState('dvic-week-page', 0);
  const [selected, setSelected] = useState<string>();
  const [settings, setSettings] = useState(false);
  const [pending, setPending] = useState<Job>();
  const request = useRef<string | null>(null);
  const today = localDate(view.dsp.timezone);
  const { start, latestDate } = selectedWeek(status.data, today, chosenWeek);
  const days = useMemo(() => weekDays(start), [start]);
  const end = days[6]!;
  // The latest reported day in this week, else the last day that has happened.
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
  const completed = jobs
    .filter((job) => job.status === 'succeeded')
    .map((job) => job.completedAt ?? '')
    .sort()
    .at(-1);
  const checked = status.data?.weeks
    .map((week) => week.checkedAt)
    .sort()
    .at(-1);
  const inspections = useInspections(
    start,
    end,
    (completed ?? '') + ':' + (checked ?? ''),
    status.data !== undefined,
  );
  const rows = useMemo(
    () => filterInspections(inspections.rows ?? [], query, vehicles),
    [inspections.rows, query, vehicles],
  );
  const dayRows = useMemo(() => rows.filter((row) => row.startDate === day), [rows, day]);
  const counts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const row of rows) counts.set(row.startDate, (counts.get(row.startDate) ?? 0) + 1);
    return counts;
  }, [rows]);
  const filtered = !!(query || vehicles);
  const shown = tab === 'day' ? dayRows : rows;
  const detail = inspections.rows?.find((row) => row.id === selected);
  const newest = jobs[0];
  const canCollect = can(view, 'dvic.collect');
  const active = Boolean(running);
  useEffect(() => setCollecting(active), [active]);
  const sync = useAction(
    async () => {
      request.current ??= randomId();
      const job = await api<Job>('/api/dsp/dvic/collect', { requestId: request.current }).catch(
        (cause) => {
          if (cause instanceof ApiError && cause.code === 'connection_required')
            throw new Error('Connect Cortex in Settings → Connections before syncing DVIC.');
          throw cause;
        },
      );
      request.current = null;
      setPending(job);
      status.refresh();
    },
    { inline: true },
  );
  const cancel = useAction(
    async () => {
      if (!running) return;
      await api('/api/dsp/dvic/jobs/' + running.id + '/cancel', {});
      status.refresh();
    },
    { inline: true },
  );
  const selectWeek = (date: string) => {
    if (date < '2000-01-02' || date > today) return;
    setChosenWeek(weekStart(date));
    setChosenDay(undefined);
    setSelected(undefined);
    setPage(0);
  };
  return (
    <div className="dvic-page">
      <Header title="DVIC">
        {can(view, 'dvic.manage') && (
          <button onClick={() => setSettings(true)}>
            <Settings2 size={16} />
            Collection settings
          </button>
        )}
        {canCollect && (
          <button
            className="primary"
            disabled={sync.busy || !!running}
            onClick={() => void sync.run()}
            title="Check the current and previous publication weeks"
          >
            <RefreshCw size={16} />
            {sync.busy || running ? 'Syncing…' : 'Sync now'}
          </button>
        )}
      </Header>
      <div className="dvic-period">
        <div className="dvic-date-controls" role="group" aria-label="Inspection week">
          <span className="muted">Inspection dates</span>
          <button
            className="icon-button"
            aria-label="Previous week"
            disabled={!status.data || start <= '2000-01-02'}
            onClick={() => selectWeek(shiftDate(start, -7))}
          >
            <ChevronLeft size={16} />
          </button>
          <span className="dvic-week-label">
            {status.data ? `${inspectionDate(start)} – ${inspectionDate(end)}` : '—'}
          </span>
          <button
            className="icon-button"
            aria-label="Next week"
            disabled={!status.data || end >= today}
            onClick={() => selectWeek(shiftDate(start, 7))}
          >
            <ChevronRight size={16} />
          </button>
          <button
            disabled={!status.data || start === weekStart(latestDate ?? today)}
            onClick={() => selectWeek(latestDate ?? today)}
            title="The most recent week with reported inspections"
          >
            Latest
          </button>
        </div>
        <span className="dvic-freshness">
          {status.data?.station || view.profile.stationCode}
          {completed || checked
            ? ' · Synced ' + time(completed || checked, view.dsp.timezone)
            : ' · No completed sync yet'}
        </span>
      </div>
      <ErrorBox message={status.error} />
      <ErrorBox message={sync.error} />
      <ErrorBox message={cancel.error} />
      {status.error && <button onClick={status.refresh}>Retry collection status</button>}
      {running && (
        <div className="dvic-sync-state" role="status">
          <span>
            {running.status === 'waiting_verification'
              ? 'Cortex needs verification in Connections.'
              : running.status === 'queued'
                ? 'DVIC sync queued'
                : running.message || 'Collecting DVIC reports…'}
          </span>
          {canCollect && (
            <button disabled={cancel.busy} onClick={() => void cancel.run()}>
              Cancel sync
            </button>
          )}
        </div>
      )}
      {!running && newest?.status === 'failed' && (
        <ErrorBox message={'Last sync failed: ' + (newest.error || newest.message)} />
      )}
      <Tabs
        label="DVIC views"
        value={tab}
        onChange={(value) => setTab(value as 'day' | 'week')}
        items={shownViews.map(([id, label]) => [id, label])}
      />
      {tab === 'day' && (
        <nav className="dvic-week-strip" aria-label="Inspection days">
          {days.map((date) => (
            <button
              key={date}
              disabled={date > today || inspections.rows === undefined}
              aria-pressed={day === date}
              onClick={() => {
                setChosenDay(date);
                setSelected(undefined);
              }}
            >
              <span>{inspectionWeekday(date)}</span>
              <strong>{Number(date.slice(8))}</strong>
              <small>
                {inspections.rows === undefined || date > today
                  ? '—'
                  : (counts.get(date) ?? 0) + ' short'}
              </small>
            </button>
          ))}
        </nav>
      )}
      <div className="dvic-filters">
        <SearchInput
          label="Search drivers"
          placeholder="Search drivers…"
          value={query}
          onChange={(value) => {
            setQuery(value);
            setPage(0);
          }}
        />
        <select
          aria-label="Vehicle type"
          value={vehicles}
          onChange={(e) => {
            setVehicles(e.target.value as VehicleClass);
            setPage(0);
          }}
        >
          <option value="">All vehicles</option>
          <option value="dot">DOT</option>
          <option value="non-dot">Non-DOT</option>
        </select>
        <span className="dvic-minimums">
          Minimums: Non-DOT (CV, CDV) · 1m 30s &nbsp; DOT (SV) · 5m
        </span>
      </div>
      <ErrorBox
        message={
          inspections.error
            ? (inspections.rows ? 'Showing previously loaded inspections. ' : '') +
              inspections.error
            : ''
        }
      />
      {inspections.error && <button onClick={inspections.refresh}>Retry inspections</button>}
      {inspections.rows === undefined ? (
        !inspections.error &&
        !status.error &&
        (status.paused || inspections.paused ? (
          <p className="muted">Inspections will load when you reconnect.</p>
        ) : (
          <Loading />
        ))
      ) : (
        <>
          {tab === 'day' ? (
            <>
              <h2 className="dvic-day-heading">
                {inspectionDate(day, true)}, {day.slice(0, 4)}
              </h2>
              <DayDigest
                rows={dayRows}
                week={rows}
                filtered={filtered}
                onSelect={(row) => setSelected(row.id)}
              />
            </>
          ) : (
            <WeekGrid
              days={days}
              rows={rows}
              today={today}
              filtered={filtered}
              page={page}
              onPageChange={setPage}
              onSelect={(row) => setSelected(row.id)}
            />
          )}
          <footer className="dvic-footer">
            <span>
              {tab === 'week'
                ? "Each cell is the driver's inspection time that day. "
                : 'Sorted shortest first. '}
              Only inspections below the required duration are collected, so this is not a
              completion rate. Dates and times are as reported by Amazon.
            </span>
            <span>
              {shown.length} {shown.length === 1 ? 'record' : 'records'}
              {tab === 'week' ? ' this week' : ''}
            </span>
          </footer>
        </>
      )}
      {detail && (
        <InspectionDetail
          row={detail}
          related={(inspections.rows ?? [])
            .filter((row) => row.driverId === detail.driverId && row.id !== detail.id)
            .sort((a, b) => b.startTime.localeCompare(a.startTime))}
          onClose={() => setSelected(undefined)}
          onSelect={(row) => setSelected(row.id)}
        />
      )}
      {settings && can(view, 'dvic.manage') && (
        <CollectionSettings onClose={() => setSettings(false)} />
      )}
    </div>
  );
}
