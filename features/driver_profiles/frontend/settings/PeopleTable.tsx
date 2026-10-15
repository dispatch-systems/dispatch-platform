import { useDeferredValue, useMemo } from 'react';
import type { Driver, DriverMatch } from '../../api/index.js';
import { useUpdateState } from '../../../../core/shell/frontend/runtime/browser-update.js';
import { useTableState } from '../../../../core/shell/frontend/runtime/useTableState.js';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import {
  DataTable,
  Empty,
  SearchInput,
  TablePagination,
  useDataTable,
} from '../../../../core/shell/frontend/ui/index.js';
import {
  driverSearchTerms,
  matchFilters,
  missing,
  roleFilters,
  type MatchFilter,
  type RoleFilter,
} from '../driver-match.js';
import { peopleColumns } from './peopleColumns.js';

/** Everyone with a code, filtered by how they are matched and by what they do. */
export function PeopleTable({
  data,
  today,
  timezone,
  onOpen,
}: {
  data: DriverMatch;
  today: string;
  timezone: string;
  onOpen: (code: string) => void;
}) {
  const [query, setQuery] = useUpdateState('driver-match-query', '');
  const [match, setMatch] = useUpdateState<MatchFilter>('driver-match-filter', 'all');
  const [role, setRole] = useUpdateState<RoleFilter>('driver-match-role', 'any');
  const state = useTableState('driver-match', { id: 'name', desc: false });
  const { drivers, sources } = data;
  // What each filter would list, counted once per answer, with what a search reads.
  const prepared = useMemo(() => {
    const matches: Record<MatchFilter, (driver: Driver) => boolean> = {
      all: () => true,
      review: (driver) => driver.review,
      missing: (driver) => missing(driver, sources),
    };
    const searchable = drivers.map((driver) => ({ driver, terms: driverSearchTerms(driver) }));
    const counts = {
      match: Object.fromEntries(
        matchFilters.map(([key]) => [key, drivers.filter(matches[key]).length]),
      ) as Record<MatchFilter, number>,
      role: Object.fromEntries(
        roleFilters.map(([key]) => [
          key,
          drivers.filter((driver) => key === 'any' || driver.role === key).length,
        ]),
      ) as Record<RoleFilter, number>,
    };
    return { matches, searchable, counts };
  }, [drivers, sources]);
  const search = query.trim().toLocaleLowerCase('en-US');
  const deferredSearch = useDeferredValue(search);
  const searching = search !== deferredSearch;
  const rows = useMemo(
    () =>
      prepared.searchable
        .filter(
          ({ driver, terms }) =>
            prepared.matches[match](driver) &&
            (role === 'any' || driver.role === role) &&
            terms.some((text) => text.includes(deferredSearch)),
        )
        .map(({ driver }) => driver),
    [prepared, match, role, deferredSearch],
  );
  const columns = useMemo(() => peopleColumns(sources, onOpen, today), [sources, onOpen, today]);
  const table = useDataTable({
    columns,
    rows,
    rowId: (driver) => driver.code,
    state,
    pageSize: 25,
  });
  return (
    <section aria-labelledby="driver-people-title">
      <div className="driver-section-head">
        <h2 id="driver-people-title">People</h2>
      </div>
      <div className="driver-tools">
        <Filters
          label="Filter by match"
          options={matchFilters}
          counts={prepared.counts.match}
          value={match}
          onChange={(value) => {
            setMatch(value);
            state.setPage(0);
          }}
        />
        <Filters
          label="Filter by role"
          options={roleFilters}
          counts={prepared.counts.role}
          value={role}
          onChange={(value) => {
            setRole(value);
            state.setPage(0);
          }}
        />
        <SearchInput
          label="Search drivers"
          placeholder="Search name, code, ID or contact"
          value={query}
          onChange={(value) => {
            setQuery(value);
            state.setPage(0);
          }}
        />
      </div>
      {drivers.length === 0 ? (
        <Empty title="No one to match yet">
          Codes appear after the first collection that names someone.
        </Empty>
      ) : (
        <div className="driver-table" aria-busy={searching} inert={searching}>
          <div className="table-wrap">
            <DataTable table={table} label="People" />
          </div>
          {!rows.length && <Empty title="No matching people">Try another name or filter.</Empty>}
          <div className="driver-table-foot">
            <span>
              {rows.length === drivers.length
                ? `${drivers.length} people`
                : `${rows.length} of ${drivers.length} people`}
              {data.checkedAt && ` · Matched ${time(data.checkedAt, timezone)}`}
            </span>
            <TablePagination table={table} variant="pages" />
          </div>
        </div>
      )}
    </section>
  );
}

/** A group of filter buttons, each with how many it lists. */
function Filters<T extends string>({
  label,
  options,
  counts,
  value,
  onChange,
}: {
  label: string;
  options: [T, string][];
  counts: Record<T, number>;
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="driver-filters" role="group" aria-label={label}>
      {options.map(([key, text]) => (
        <button key={key} aria-pressed={value === key} onClick={() => onChange(key)}>
          {text}
          <span>{counts[key]}</span>
        </button>
      ))}
    </div>
  );
}
