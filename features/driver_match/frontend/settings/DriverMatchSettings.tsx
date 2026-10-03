import { useDeferredValue, useMemo, useState } from 'react';
import { useUpdateState } from '../../app/browser-update.js';
import { useDriverMatch } from '../../app/endpoints.js';
import { useTableState } from '../../app/useTableState.js';
import { dateFormatter } from '../../lib/date-format.js';
import { time } from '../../lib/format.js';
import {
  dataLabels,
  dataOrder,
  driverFilters,
  driverFilter,
  driverSearchTerms,
  type DriverFilter,
} from '../../lib/driver-match.js';
import {
  DataState,
  DataTable,
  Empty,
  SearchInput,
  TablePagination,
  useDataTable,
} from '../../ui/index.js';
import { dataIcons } from './dataIcons.js';
import { driverColumns } from './driverColumns.js';
import { DriverSheet } from './DriverSheet.js';
import { DriverSummary } from './DriverSummary.js';
import { ReviewCard } from './ReviewCard.js';

/** Settings → Driver Match: who might be listed twice, then everyone with a code. */
export function DriverMatchSettings({ timezone }: { timezone: string }) {
  const request = useDriverMatch();
  const [query, setQuery] = useUpdateState('driver-match-query', '');
  const [filter, setFilter] = useUpdateState<DriverFilter>('driver-match-filter', 'all');
  const [open, setOpen] = useState<string | null>(null);
  const state = useTableState('driver-match', { id: 'name', desc: false });
  const today = dateFormatter('en-CA', { timeZone: timezone }).format(new Date());
  const drivers = useMemo(() => request.data?.drivers ?? [], [request.data]);
  const prepared = useMemo(() => {
    const counts = Object.fromEntries(driverFilters.map(([key]) => [key, 0])) as Record<
      DriverFilter,
      number
    >;
    counts.all = drivers.length;
    const searchable = drivers.map((driver) => {
      const kind = driverFilter(driver);
      counts[kind]++;
      return { driver, kind, terms: driverSearchTerms(driver) };
    });
    return { counts, searchable };
  }, [drivers]);
  const search = query.trim().toLocaleLowerCase('en-US');
  const deferredSearch = useDeferredValue(search);
  const searching = search !== deferredSearch;
  const rows = useMemo(
    () =>
      prepared.searchable
        .filter(
          ({ kind, terms }) =>
            (filter === 'all' || kind === filter) &&
            terms.some((text) => text.includes(deferredSearch)),
        )
        .map(({ driver }) => driver),
    [prepared, filter, deferredSearch],
  );
  const columns = useMemo(() => driverColumns(setOpen, today), [today]);
  const table = useDataTable({
    columns,
    rows,
    rowId: (driver) => driver.code,
    state,
    pageSize: 25,
  });
  return (
    <div className="driver-match">
      <DataState data={request.data} error={request.error}>
        {(data) => (
          <>
            <DriverSummary data={data} />
            {data.review.length > 0 && <ReviewCard pairs={data.review} onOpen={setOpen} />}
            <section aria-labelledby="driver-list-title">
              <div className="driver-section-head">
                <h2 id="driver-list-title">All drivers</h2>
                <div className="driver-legend" aria-hidden="true">
                  {dataOrder.map((kind) => {
                    const Icon = dataIcons[kind];
                    return (
                      <span key={kind}>
                        <Icon size={13} />
                        {dataLabels[kind]}
                      </span>
                    );
                  })}
                </div>
              </div>
              <div className="driver-tools">
                <div className="driver-filters" role="group" aria-label="Filter drivers">
                  {driverFilters.map(([key, label]) => (
                    <button
                      key={key}
                      className={key === 'review' ? 'review' : undefined}
                      aria-pressed={filter === key}
                      onClick={() => {
                        setFilter(key);
                        state.setPage(0);
                      }}
                    >
                      {label}
                      <span>{prepared.counts[key]}</span>
                    </button>
                  ))}
                </div>
                <SearchInput
                  label="Search drivers"
                  placeholder="Search name, code or ID"
                  value={query}
                  onChange={(value) => {
                    setQuery(value);
                    state.setPage(0);
                  }}
                />
              </div>
              {data.drivers.length === 0 ? (
                <Empty title="No one to match yet">
                  Codes appear after the first Paycom or Amazon collection.
                </Empty>
              ) : (
                <div className="driver-table" aria-busy={searching} inert={searching}>
                  <div className="table-wrap">
                    <DataTable table={table} label="Drivers" />
                  </div>
                  {!rows.length && (
                    <Empty title="No matching drivers">Try another name or filter.</Empty>
                  )}
                  <div className="driver-table-foot">
                    <span>
                      {rows.length === drivers.length
                        ? `${drivers.length} people`
                        : `${rows.length} of ${drivers.length} people`}
                      {data.checkedAt && ` · Checked ${time(data.checkedAt, timezone)}`}
                    </span>
                    <TablePagination table={table} variant="pages" />
                  </div>
                </div>
              )}
            </section>
          </>
        )}
      </DataState>
      {open && (
        <DriverSheet
          code={open}
          drivers={drivers}
          timezone={timezone}
          today={today}
          onClose={() => setOpen(null)}
        />
      )}
    </div>
  );
}
