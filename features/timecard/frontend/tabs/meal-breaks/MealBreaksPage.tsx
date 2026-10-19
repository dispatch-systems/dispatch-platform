import { performancePolicy } from '../../../../../core/shell/frontend/lib/performance-policy.js';
import { useUpdateState } from '../../../../../core/shell/frontend/runtime/browser-update.js';
import { useDeferredValue, useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ArrowRight, Download, Globe, Info, RefreshCw } from 'lucide-react';
import {
  mealPageUrl,
  exportMealComparison,
  useMealComparison,
  useMealPeriodGaps,
} from '../../../api/client.js';
import { navigate } from '../../../../../core/shell/frontend/runtime/navigation.js';
import { useTableState } from '../../../../../core/shell/frontend/runtime/useTableState.js';
import {
  DataState,
  DataTable,
  downloadTable,
  Empty,
  ErrorBox,
  SearchInput,
  TablePagination,
  useDataTable,
} from '../../../../../core/shell/frontend/ui/index.js';
import { time } from '../../../../../core/shell/frontend/lib/format.js';
import { clockLabel, displayMeal, personName } from '../../meal-breaks.js';
import type { MealEmployee, PaycomPreferences } from '../../../api/index.js';
import { PaycomDateControls } from '../../DateControls.js';
import { mealColumns, mealLines, type MealLine } from './mealColumns.js';
import { useAdjacentDays } from '../../useAdjacentDays.js';
import { MealDetail } from './MealGapSummary.js';
import './meal-breaks.css';

const rowClassName = (_: MealLine, { depth }: { depth: number }) =>
  depth ? 'meal-extra' : undefined;

export function MealBreaksPage({
  date,
  today,
  onDateChange,
  refreshKey,
  timezone,
  driverMatch,
  preferences,
  onOpenEmployee,
}: {
  date: string;
  today: string;
  onDateChange: (date: string) => void;
  refreshKey?: string | null;
  timezone: string;
  /** Driver Match's page, while the person may open it. */
  driverMatch?: string;
  preferences: PaycomPreferences;
  onOpenEmployee?: (code: string, date: string) => void;
}) {
  const [query, setQuery] = useUpdateState('meal-query', ''),
    [filter, setFilter] = useUpdateState('meal-filter', 'all');
  const state = useTableState('meal', { id: 'employee', desc: false });
  const [settledQuery, setSettledQuery] = useState(query);
  useEffect(() => {
    const timer = setTimeout(() => setSettledQuery(query), performancePolicy.employeeSearchDelayMs);
    return () => clearTimeout(timer);
  }, [query]);
  const deferredSearch = useDeferredValue(settledQuery.toLowerCase());
  const searching = deferredSearch !== query.toLowerCase();
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState('');
  const url = mealPageUrl(
    date,
    deferredSearch,
    filter,
    state.sort?.id,
    state.sort?.desc,
    state.page,
  );
  const request = useMealComparison(url, refreshKey);
  useAdjacentDays(url, date, today, request.data);
  const current = request.data?.date === date ? request.data : undefined;
  // The previous day's rows hold the layout, dimmed and inert, until the new day arrives.
  const data = current ?? request.stale;
  const shownDate = data?.date ?? date;
  const zone = data?.timezone ?? timezone;
  const gapsRequest = useMealPeriodGaps(date, refreshKey);
  // Whichever answer's period holds the day shown: the previous one keeps its counts while
  // a day of the same period loads.
  const periodGaps = [gapsRequest.data, gapsRequest.stale].find(
    (gaps) => gaps?.period && gaps.period.from <= shownDate && shownDate <= gaps.period.to,
  );
  const { page, setPage } = state;
  useEffect(() => {
    if (current && page && page * 100 >= current.total)
      setPage(Math.max(0, Math.ceil(current.total / 100) - 1));
  }, [current, page, setPage]);
  const nameOrder = preferences.name_order;
  const lateTime = preferences.late_da_time,
    lateDepartments = preferences.late_da_departments;
  const rows = useMemo<MealLine[]>(
    () =>
      (data?.rows ?? []).map((row: MealEmployee) => {
        const count = periodGaps?.counts[row.id],
          period = periodGaps?.period;
        return {
          ...mealLines(row, displayMeal(row), personName(row.name, nameOrder), shownDate),
          onOpenEmployee:
            onOpenEmployee && row.id.startsWith('paycom:')
              ? () => onOpenEmployee(row.id.slice(7), shownDate)
              : undefined,
          periodGaps: count && period ? { count, period } : undefined,
        };
      }),
    [data, shownDate, nameOrder, onOpenEmployee, periodGaps],
  );
  const counts = data?.counts ?? { all: 0, late: 0, different: 0, missing: 0, gaps: 0 };
  const filtered = rows;
  const table = useDataTable({
    columns: mealColumns,
    rows: filtered,
    total: data?.total,
    sorting: 'server',
    rowId: (line) => line.id,
    state,
    pageSize: 100,
    subRows: (line) => line.more,
  });
  const unlinked = data?.drivers.filter((d) => !d.paycomCode).length ?? 0;
  const zones = new Set(data?.cortexPublications.map((p) => p.timezone));
  return (
    <section className="meal-page" aria-labelledby="meal-heading">
      <header className="paycom-table-heading paycom-timecard-heading meal-heading">
        <div className="paycom-timecard-title">
          <h2 id="meal-heading">Meal Breaks</h2>
          {data && (
            <span className="paycom-employee-count">
              {data.total}
              {data.total !== counts.all && ` of ${counts.all}`}{' '}
              {counts.all === 1 ? 'employee' : 'employees'}
            </span>
          )}
        </div>
        <PaycomDateControls
          date={date}
          today={today}
          onChange={(value) => {
            onDateChange(value);
            setPage(0);
            table.collapseAll();
          }}
        />
        <button
          className="icon-button"
          aria-label="Export meal breaks"
          disabled={!current?.total || searching || exporting}
          aria-busy={exporting}
          onClick={async () => {
            setExporting(true);
            setExportError('');
            try {
              const full = await exportMealComparison(
                shownDate,
                deferredSearch,
                filter,
                state.sort?.id ?? 'employee',
                state.sort?.desc ?? false,
              );
              const allRows = full.rows.map((row) =>
                mealLines(row, displayMeal(row), personName(row.name, nameOrder), shownDate),
              );
              downloadTable({ ...table, allRows }, `meal-breaks-${shownDate}.csv`);
            } catch (error) {
              setExportError(error instanceof Error ? error.message : String(error));
            } finally {
              setExporting(false);
            }
          }}
        >
          <Download size={16} />
        </button>
      </header>
      {exportError && <ErrorBox message={exportError} />}
      <div className="meal-toolbar">
        <SearchInput
          label="Search meal break employees"
          placeholder="Search employees…"
          value={query}
          onChange={(value) => {
            setQuery(value);
            setPage(0);
          }}
        />
        <div className="meal-filters" aria-label="Filter meal breaks">
          {(
            [
              ['all', 'All'],
              ['late', 'Late DAs'],
              ['different', 'Different times'],
              ['missing', 'Missing data'],
              ['gaps', 'Gaps > 5 min'],
            ] as const
          ).map(([key, label]) => (
            <button
              key={key}
              className={key === 'gaps' && counts.gaps > 0 ? 'meal-gap-filter' : undefined}
              aria-pressed={filter === key}
              onClick={() => {
                setFilter(key);
                setPage(0);
              }}
            >
              {key === 'gaps' && counts.gaps > 0 && <AlertTriangle size={15} aria-hidden="true" />}
              {label}
              <span>{counts[key]}</span>
            </button>
          ))}
        </div>
        <button
          className="icon-button meal-refresh"
          aria-label="Refresh meal breaks"
          onClick={request.refresh}
        >
          <RefreshCw size={16} />
        </button>
      </div>
      <ErrorBox message={request.error} />
      {request.error && data && (
        <p className="meal-load-notice" role="status">
          Showing the last loaded results. Refresh to try again.
        </p>
      )}
      {data && unlinked > 0 && (
        <div className="meal-link-notice" inert={!current}>
          <span>
            {unlinked} Flex {unlinked === 1 ? 'driver is' : 'drivers are'} not matched to a Paycom
            employee and {unlinked === 1 ? 'appears' : 'appear'} on their own.
          </span>
          {driverMatch && (
            <button className="text-button" onClick={() => navigate(driverMatch)}>
              Review in Driver Match
              <ArrowRight size={15} />
            </button>
          )}
        </div>
      )}
      <DataState data={data} failed={Boolean(request.error)}>
        {(data) =>
          !data.rows.length ? (
            <Empty title="No meal breaks or punches for this date">
              {!data.paycomCollectedAt && !data.cortexPublications.length
                ? 'Neither source has a collection for this date.'
                : 'Choose another date to compare collected records.'}
            </Empty>
          ) : (
            <div
              className="paycom-day-results"
              aria-busy={!current || searching}
              inert={!current || searching}
            >
              {(!data.paycomCollectedAt || !data.cortexPublications.length) && (
                <p className="meal-source-notice" role="status">
                  <AlertTriangle size={16} aria-hidden="true" />
                  <span>
                    {!data.paycomCollectedAt
                      ? 'Paycom has no collection for this date. Showing Flex records.'
                      : 'Flex has no collection for this date. Showing Paycom records.'}
                  </span>
                </p>
              )}
              <div
                className="meal-table-scroll"
                role="region"
                aria-label="Meal break comparison"
                tabIndex={0}
              >
                <DataTable
                  table={table}
                  stickyHeader
                  className="meal-table"
                  caption={`Meal breaks for ${shownDate}. Paycom local clock times and Flex station-local times, compared to the minute.`}
                  rowClassName={rowClassName}
                  renderDetail={(line) => <MealDetail line={line} refreshKey={refreshKey} />}
                  detailClassName="meal-detail"
                />
              </div>
              {!filtered.length && (
                <Empty title="No matching employees">Try another name or filter.</Empty>
              )}
              <TablePagination table={table} variant="pages" />
            </div>
          )
        }
      </DataState>
      <footer className="paycom-timecard-footer" aria-label="Meal break timezones">
        <span>
          <Globe size={16} aria-hidden="true" />
          {zones.size > 1 ? 'Local time for each Flex station' : zone.replaceAll('_', ' ')}
        </span>
        <div className="paycom-timecard-business-time">
          <details className="paycom-timecard-info">
            <summary aria-label="About meal break data">
              <Info size={16} aria-hidden="true" />
            </summary>
            <p>
              {data ? (
                <>
                  Paycom collected:{' '}
                  {data.paycomCollectedAt ? time(data.paycomCollectedAt, zone) : 'No collection'}.
                  <br />
                  Flex collected:{' '}
                  {data.cortexPublications[0]
                    ? time(data.cortexPublications[0].collectedAt, zone)
                    : 'No collection'}
                  .<br />
                  <br />
                </>
              ) : null}
              Employees with a Flex meal or any Paycom punch on this date.
              <br />
              <br />
              Differences use displayed minutes: Flex minus Paycom. Paycom punches use their
              recorded local clock time; Flex times use the station’s timezone. A missing value is
              shown as —.
              <br />
              <br />
              Delivery gaps use Flex only: last delivery → OUT LUNCH, and IN LUNCH → first delivery.
              Only gaps over 5 minutes are flagged.
              <br />
              <br />
              Late DAs have a Paycom IN DAY punch at or after {clockLabel(lateTime)}
              {lateDepartments.length > 0 &&
                ` in ${lateDepartments.map((d) => d || 'No department').join(', ')}`}
              .
            </p>
          </details>
        </div>
      </footer>
    </section>
  );
}
