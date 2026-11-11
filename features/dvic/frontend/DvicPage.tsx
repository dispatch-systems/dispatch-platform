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
  const dvic = useDvicPage(view);
  return (
    <div className="dvic-page">
      <Header title="DVIC">
        {dvic.settings && (
          <button onClick={() => navigate(dvic.settings)}>
            <Settings2 size={16} />
            Collection settings
          </button>
        )}
        {dvic.canCollect && (
          <button
            className="primary"
            disabled={dvic.sync.busy || !!dvic.running}
            onClick={() => void dvic.sync.run()}
            title="Check the latest publication weeks and the week shown"
          >
            <RefreshCw size={16} />
            {dvic.sync.busy || dvic.running ? 'Syncing…' : 'Sync now'}
          </button>
        )}
      </Header>
      <div className="dvic-period">
        <div className="dvic-date-controls" role="group" aria-label="Inspection week">
          <span className="muted">Inspection dates</span>
          <button
            className="icon-button"
            aria-label="Previous week"
            disabled={!dvic.status.data || dvic.start <= '2000-01-02'}
            onClick={() => dvic.selectWeek(shiftDate(dvic.start, -7))}
          >
            <ChevronLeft size={16} />
          </button>
          <span className="dvic-week-label">
            {dvic.status.data ? `${inspectionDate(dvic.start)} – ${inspectionDate(dvic.end)}` : '—'}
          </span>
          <button
            className="icon-button"
            aria-label="Next week"
            disabled={!dvic.status.data || dvic.end >= dvic.today}
            onClick={() => dvic.selectWeek(shiftDate(dvic.start, 7))}
          >
            <ChevronRight size={16} />
          </button>
          <button
            disabled={!dvic.status.data || dvic.start === weekStart(dvic.latestDate ?? dvic.today)}
            onClick={() => dvic.selectWeek(dvic.latestDate ?? dvic.today)}
            title="The most recent collected week"
          >
            Latest
          </button>
          <button
            disabled={
              !dvic.status.data ||
              (dvic.start === weekStart(dvic.today) &&
                (dvic.tab === 'week' || dvic.day === dvic.today))
            }
            onClick={() => dvic.selectWeek(dvic.today, dvic.today)}
            title="Today's inspections"
          >
            Today
          </button>
        </div>
        <span className="dvic-freshness">
          {dvic.status.data?.station || view.profile.stationCode}
          {dvic.completed || dvic.checked
            ? ' · Synced ' + time(dvic.completed || dvic.checked, view.dsp.timezone)
            : ' · No completed sync yet'}
        </span>
      </div>
      <ErrorBox message={dvic.status.error} />
      <ErrorBox message={dvic.sync.error} />
      <ErrorBox message={dvic.cancel.error} />
      {dvic.status.error && <button onClick={dvic.status.refresh}>Retry collection status</button>}
      {dvic.running && (
        <div className="dvic-sync-state" role="status">
          <span>
            {dvic.running.status === 'waiting_verification'
              ? 'Cortex needs verification in Connections.'
              : dvic.running.status === 'queued'
                ? 'DVIC sync queued'
                : dvic.running.message || 'Collecting DVIC reports…'}
          </span>
          {dvic.canCollect && (
            <button disabled={dvic.cancel.busy} onClick={() => void dvic.cancel.run()}>
              Cancel sync
            </button>
          )}
        </div>
      )}
      {!dvic.running && dvic.newest?.status === 'failed' && (
        <ErrorBox message={'Last sync failed: ' + (dvic.newest.error || dvic.newest.message)} />
      )}
      <Tabs
        label="DVIC views"
        value={dvic.tabs.open?.id ?? dvic.tab}
        onChange={(id) => dvic.tabs.change(id, (own) => dvic.setTab(own as 'day' | 'week'))}
        onIntent={dvic.tabs.warm}
        items={dvic.tabs.items}
      />
      {dvic.tabs.open ? (
        <HostedTab tab={dvic.tabs.open} />
      ) : (
        <>
          {dvic.tab === 'day' && (
            <nav className="dvic-week-strip" aria-label="Inspection days">
              {dvic.days.map((date) => (
                <button
                  key={date}
                  disabled={date > dvic.today || dvic.summary === undefined}
                  aria-pressed={dvic.day === date}
                  onClick={() => dvic.selectDay(date)}
                >
                  <span>{inspectionWeekday(date)}</span>
                  <strong>{Number(date.slice(8))}</strong>
                  <small>
                    {dvic.summary === undefined || date > dvic.today
                      ? '—'
                      : dvic.counts.get(date) || dvic.collected(date)
                        ? (dvic.counts.get(date) ?? 0) + ' short'
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
              value={dvic.query}
              onChange={dvic.changeQuery}
            />
            <select
              aria-label="Vehicle type"
              value={dvic.vehicles}
              onChange={(e) => dvic.changeVehicles(e.target.value as VehicleClass)}
            >
              <option value="">All vehicles</option>
              <option value="dot">DOT</option>
              <option value="non-dot">Non-DOT</option>
            </select>
            <span className="dvic-minimums">
              Minimums: Non-DOT (Cargo Van, CDV, EDV) · 1m 30s &nbsp; DOT (Step Van) · 5m
            </span>
          </div>
          <ErrorBox
            message={
              dvic.inspections.error
                ? (dvic.summary ? 'Showing previously loaded inspections. ' : '') +
                  dvic.inspections.error
                : ''
            }
          />
          {dvic.inspections.error && (
            <button onClick={dvic.inspections.refresh}>Retry inspections</button>
          )}
          {dvic.summary === undefined ? (
            !dvic.inspections.error &&
            !dvic.status.error &&
            (dvic.status.paused || dvic.inspections.paused ? (
              <p className="muted">Inspections will load when you reconnect.</p>
            ) : (
              <Loading />
            ))
          ) : (
            <>
              {dvic.tab === 'day' ? (
                <>
                  <h2 className="dvic-day-heading">
                    {inspectionDate(dvic.day, true)}, {dvic.day.slice(0, 4)}
                  </h2>
                  <DayDigest
                    day={dvic.day}
                    summary={dvic.daySummary}
                    query={dvic.search}
                    publication={dvic.publication}
                    filtered={dvic.filtered}
                    collected={dvic.collected(dvic.day)}
                    onSelect={(row) => dvic.setSelected(row.id)}
                  />
                </>
              ) : (
                <WeekGrid
                  days={dvic.days}
                  summary={dvic.summary}
                  today={dvic.today}
                  collected={dvic.collected}
                  filtered={dvic.filtered}
                  page={dvic.page}
                  onPageChange={dvic.setPage}
                  onSelect={dvic.setSelected}
                />
              )}
              <footer className="dvic-footer">
                <span>
                  {dvic.tab === 'week'
                    ? "Each cell is the driver's inspection time that day. "
                    : 'Sorted shortest first. '}
                  Only inspections below the required duration are collected, so this is not a
                  completion rate. Dates and times are as reported by Amazon.
                </span>
                <span>
                  {dvic.shown} {dvic.shown === 1 ? 'record' : 'records'}
                  {dvic.tab === 'week' ? ' this week' : ''}
                </span>
              </footer>
            </>
          )}
        </>
      )}
      {dvic.selected && (
        <InspectionDetail
          id={dvic.selected}
          from={dvic.start}
          to={dvic.end}
          publication={dvic.publication}
          onClose={() => dvic.setSelected(undefined)}
          onSelect={dvic.setSelected}
        />
      )}
    </div>
  );
}
