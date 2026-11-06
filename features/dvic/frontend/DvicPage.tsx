import { ChevronLeft, ChevronRight, RefreshCw, Settings2 } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import { navigate } from '../../../core/shell/frontend/runtime/navigation.js';
import { inspectionDate, inspectionWeekday, weekStart, type VehicleClass } from './dvic.js';
import { shiftDate } from '../../../core/shell/frontend/lib/day.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import {
  ErrorBox,
  Header,
  Loading,
  SearchInput,
  Tabs,
} from '../../../core/shell/frontend/ui/index.js';
import { HostedTab } from '../../../core/shell/frontend/runtime/PageTabs.js';
import { DayDigest } from './tabs/day/DayDigest.js';
import { InspectionDetail } from './InspectionDetail.js';
import { WeekGrid } from './tabs/week/WeekGrid.js';
import { useDvicPage } from './useDvicPage.js';

export function DvicPage({ view }: { view: DspView }) {
  const {
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
  } = useDvicPage(view);
  return (
    <div className="dvic-page">
      <Header title="DVIC">
        {settings && (
          <button onClick={() => navigate(settings)}>
            <Settings2 size={16} />
            Collection settings
          </button>
        )}
        {canCollect && (
          <button
            className="primary"
            disabled={sync.busy || !!running}
            onClick={() => void sync.run()}
            title="Check the latest publication weeks and the week shown"
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
            title="The most recent collected week"
          >
            Latest
          </button>
          <button
            disabled={
              !status.data || (start === weekStart(today) && (tab === 'week' || day === today))
            }
            onClick={() => selectWeek(today, today)}
            title="Today's inspections"
          >
            Today
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
        value={tabs.open?.id ?? tab}
        onChange={(id) => tabs.change(id, (own) => setTab(own as 'day' | 'week'))}
        onIntent={tabs.warm}
        items={tabs.items}
      />
      {tabs.open ? (
        <HostedTab tab={tabs.open} />
      ) : (
        <>
          {tab === 'day' && (
            <nav className="dvic-week-strip" aria-label="Inspection days">
              {days.map((date) => (
                <button
                  key={date}
                  disabled={date > today || summary === undefined}
                  aria-pressed={day === date}
                  onClick={() => selectDay(date)}
                >
                  <span>{inspectionWeekday(date)}</span>
                  <strong>{Number(date.slice(8))}</strong>
                  <small>
                    {summary === undefined || date > today
                      ? '—'
                      : counts.get(date) || collected(date)
                        ? (counts.get(date) ?? 0) + ' short'
                        : 'Not collected'}
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
              onChange={changeQuery}
            />
            <select
              aria-label="Vehicle type"
              value={vehicles}
              onChange={(e) => changeVehicles(e.target.value as VehicleClass)}
            >
              <option value="">All vehicles</option>
              <option value="dot">DOT</option>
              <option value="non-dot">Non-DOT</option>
            </select>
            <span className="dvic-minimums">
              Minimums: Non-DOT (CV, CDV, EDV) · 1m 30s &nbsp; DOT (SV) · 5m
            </span>
          </div>
          <ErrorBox
            message={
              inspections.error
                ? (summary ? 'Showing previously loaded inspections. ' : '') + inspections.error
                : ''
            }
          />
          {inspections.error && <button onClick={inspections.refresh}>Retry inspections</button>}
          {summary === undefined ? (
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
                    day={day}
                    summary={daySummary}
                    query={search}
                    publication={publication}
                    filtered={filtered}
                    collected={collected(day)}
                    onSelect={(row) => setSelected(row.id)}
                  />
                </>
              ) : (
                <WeekGrid
                  days={days}
                  summary={summary}
                  today={today}
                  collected={collected}
                  filtered={filtered}
                  page={page}
                  onPageChange={setPage}
                  onSelect={setSelected}
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
                  {shown} {shown === 1 ? 'record' : 'records'}
                  {tab === 'week' ? ' this week' : ''}
                </span>
              </footer>
            </>
          )}
        </>
      )}
      {selected && (
        <InspectionDetail
          id={selected}
          from={start}
          to={end}
          publication={publication}
          onClose={() => setSelected(undefined)}
          onSelect={setSelected}
        />
      )}
    </div>
  );
}
