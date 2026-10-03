import { useUpdateState } from '../../../../../core/shell/frontend/runtime/browser-update.js';
import { useDeferredValue, useMemo } from 'react';
import { AlertTriangle, ArrowRight, Download, Globe, Info, RefreshCw } from 'lucide-react';
import { mealComparisonUrl, useMealComparison } from '../../../api/client.js';
import { dspHash, navigate } from '../../../../../core/shell/frontend/runtime/navigation.js';
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
import { personName, time } from '../../../../../core/shell/frontend/lib/format.js';
import { clockLabel, displayMeal } from '../../meal-breaks.js';
import { type MealEmployee } from '../../../../../shared/contracts/meals.js';
import type { PaycomPreferences } from '../../../../../shared/contracts/paycom.js';
import { PaycomDateControls } from '../../DateControls.js';
import { MealDetail, mealColumns, mealLines, type MealLine } from './mealColumns.js';
import { useAdjacentDays } from '../../useAdjacentDays.js';
import './meal-breaks.css';

const rowClassName = (_: MealLine, { depth }: { depth: number }) =>
  depth ? 'meal-extra' : undefined;
const renderDetail = (line: MealLine) => <MealDetail line={line} />;

export function MealBreaksPage({
  date,
  today,
  onDateChange,
  refreshKey,
  timezone,
  dspId,
  canMatch,
  preferences,
}: {
  date: string;
  today: string;
  onDateChange: (date: string) => void;
  refreshKey?: string | null;
  timezone: string;
  dspId: string;
  /** Whether the person may review matches in Settings → Driver Match. */
  canMatch: boolean;
  preferences: PaycomPreferences;
}) {
  const [query, setQuery] = useUpdateState('meal-query', ''),
    [filter, setFilter] = useUpdateState('meal-filter', 'all');
  const state = useTableState('meal', { id: 'employee', desc: false });
  const url = mealComparisonUrl(date);
  const request = useMealComparison(date, refreshKey);
  useAdjacentDays(url, date, today, request.data);
  const current = request.data?.date === date ? request.data : undefined;
  // The previous day's rows hold the layout, dimmed and inert, until the new day arrives.
  const data = current ?? request.stale;
  const shownDate = data?.date ?? date;
  const zone = data?.timezone ?? timezone;
  const nameOrder = preferences.name_order;
  const lateTime = preferences.late_da_time,
    lateDepartments = preferences.late_da_departments;
  const rows = useMemo(
    () =>
      (data?.rows ?? []).map((row: MealEmployee) =>
        mealLines(row, displayMeal(row), personName(row.name, nameOrder), shownDate),
      ),
    [data, shownDate, nameOrder],
  );
  const prepared = useMemo(() => {
    const counts = { all: rows.length, late: 0, different: 0, missing: 0, gaps: 0 };
    const searchable = rows.map((line) => {
      counts.late += Number(line.summary.lateIn);
      counts.different += Number(line.summary.different);
      counts.missing += Number(line.summary.missing);
      counts.gaps += Number(line.summary.longGap);
      return {
        line,
        text: `${line.name} ${line.row.paycom?.employeeCode ?? ''} ${line.row.cortex.map((m) => m.driverName).join(' ')}`.toLowerCase(),
      };
    });
    return { counts, searchable };
  }, [rows]);
  const counts = prepared.counts;
  const search = query.toLowerCase();
  const deferredSearch = useDeferredValue(search);
  const searching = deferredSearch !== search;
  const filtered = useMemo(
    () =>
      prepared.searchable
        .filter(
          ({ line: { summary }, text }) =>
            (filter === 'all' ||
              (filter === 'late'
                ? summary.lateIn
                : filter === 'different'
                  ? summary.different
                  : filter === 'gaps'
                    ? summary.longGap
                    : summary.missing)) &&
            text.includes(deferredSearch),
        )
        .map(({ line }) => line),
    [prepared, filter, deferredSearch],
  );
  const table = useDataTable({
    columns: mealColumns,
    rows: filtered,
    rowId: (line) => line.id,
    state,
    pageSize: 100,
    subRows: (line) => line.more,
  });
  const setPage = state.setPage;
  const unlinked = data?.drivers.filter((d) => d.matchType === 'unmatched').length ?? 0;
  const zones = new Set(data?.cortexPublications.map((p) => p.timezone));
  return (
    <section className="meal-page" aria-labelledby="meal-heading">
      <header className="paycom-table-heading paycom-timecard-heading meal-heading">
        <div className="paycom-timecard-title">
          <h2 id="meal-heading">Meal Breaks</h2>
          {data && (
            <span className="paycom-employee-count">
              {filtered.length}
              {filtered.length !== counts.all && ` of ${counts.all}`}{' '}
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
          disabled={!filtered.length || searching}
          onClick={() => downloadTable(table, `meal-breaks-${shownDate}.csv`)}
        >
          <Download size={16} />
        </button>
      </header>
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
          {canMatch && (
            <button
              className="text-button"
              onClick={() => navigate(dspHash(dspId, 'settings', { tab: 'driver-match' }))}
            >
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
                  renderDetail={renderDetail}
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
