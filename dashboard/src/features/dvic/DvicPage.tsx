import { useEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, RefreshCw, Settings2 } from 'lucide-react';
import type { DspView, Job } from '../../../../shared/contracts/index.js';
import type { DvicStatus } from '../../../../shared/contracts/dvic.js';
import { api, ApiError, useData } from '../../app/api.js';
import { can } from '../../app/permissions.js';
import { useAction } from '../../app/useAction.js';
import {
  filterInspections,
  inspectionDate,
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
import { useInspections } from './useInspections.js';

const activeJob = (job: Job) => ['queued', 'running', 'waiting_verification'].includes(job.status);

export function DvicPage({ view }: { view: DspView }) {
  const status = useData<DvicStatus>('/api/dsp/dvic/status', performancePolicy.recoveryPollMs);
  const [tab, setTab] = useState<'day' | 'week'>('day');
  const [chosenWeek, setChosenWeek] = useState<string>();
  const [chosenDay, setChosenDay] = useState<string>();
  const [query, setQuery] = useState('');
  const [vehicles, setVehicles] = useState<VehicleClass>('');
  const [selected, setSelected] = useState<string>();
  const [settings, setSettings] = useState(false);
  const [pending, setPending] = useState<Job>();
  const request = useRef<string | null>(null);
  const today = localDate(view.dsp.timezone);
  const latestDate = status.data?.reports
    .map((r) => r.maxDate)
    .filter((date): date is string => !!date && date <= today)
    .sort()
    .at(-1);
  const start = chosenWeek ?? weekStart(latestDate ?? today);
  const days = weekDays(start);
  const end = days[6]!;
  // The latest reported day in this week, else the last day that has happened.
  const day =
    chosenDay ??
    (latestDate && latestDate >= start && latestDate <= end
      ? latestDate
      : ([...days].reverse().find((date) => date <= today) ?? start));
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
  const inspections = useInspections(start, end, (completed ?? '') + ':' + (checked ?? ''));
  const rows = filterInspections(inspections.rows ?? [], query, vehicles);
  const dayRows = rows.filter((row) => row.startDate === day);
  const filtered = !!(query || vehicles);
  const shown = tab === 'day' ? dayRows : rows;
  const detail = inspections.rows?.find((row) => row.id === selected);
  const newest = jobs[0];
  const canCollect = can(view, 'dvic.collect');
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(status.refresh, performancePolicy.activeCollectionPollMs);
    return () => clearInterval(timer);
  }, [running?.id, status.refresh]);
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
            disabled={start <= '2000-01-02'}
            onClick={() => selectWeek(shiftDate(start, -7))}
          >
            <ChevronLeft size={16} />
          </button>
          <span className="dvic-week-label">
            {inspectionDate(start)} – {inspectionDate(end)}
          </span>
          <button
            className="icon-button"
            aria-label="Next week"
            disabled={end >= today}
            onClick={() => selectWeek(shiftDate(start, 7))}
          >
            <ChevronRight size={16} />
          </button>
          <button
            disabled={start === weekStart(latestDate ?? today)}
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
        items={[
          ['day', 'Day'],
          ['week', 'Week'],
        ]}
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
              <span>
                {new Intl.DateTimeFormat('en-US', { timeZone: 'UTC', weekday: 'short' }).format(
                  new Date(date + 'T12:00:00Z'),
                )}
              </span>
              <strong>{Number(date.slice(8))}</strong>
              <small>
                {inspections.rows === undefined || date > today
                  ? '—'
                  : rows.filter((row) => row.startDate === date).length + ' short'}
              </small>
            </button>
          ))}
        </nav>
      )}
      <div className="dvic-filters">
        <SearchInput
          label="Search inspections"
          placeholder="Search driver or VIN…"
          value={query}
          onChange={setQuery}
        />
        <select
          aria-label="Vehicle type"
          value={vehicles}
          onChange={(e) => setVehicles(e.target.value as VehicleClass)}
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
        !inspections.error && <Loading />
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
