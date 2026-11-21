import { useMemo, useState } from 'react';
import { ArrowDown, ArrowUp, ArrowUpDown } from 'lucide-react';
import { Empty, SortHeader } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview } from '../../api/index.js';
import { Person, RowLink, TierBadge, categoryName, personOf } from '../parts.js';
import { printedDay } from '../tiers.js';
import { indexOf } from '../data.js';
import { type HistoryFilters } from '../../api/client.js';
import { useListScroll } from '../useListScroll.js';
import { useConductFilters } from '../useConductFilters.js';

/** What a row is sorted by. */
type Row = { person: { name: string }; count: number; last: string; waiting: boolean };
type Direction = 'asc' | 'desc';

/** The columns that sort the list: each one's heading, ascending order and first click. */
const columns = {
  name: {
    label: 'Driver',
    ascending: (a: Row, b: Row) => a.person.name.localeCompare(b.person.name),
    first: 'asc',
  },
  count: { label: 'Writeups', ascending: (a: Row, b: Row) => a.count - b.count, first: 'desc' },
  last: {
    label: 'Last writeup',
    ascending: (a: Row, b: Row) => a.last.localeCompare(b.last),
    first: 'desc',
  },
  waiting: {
    label: 'Signature',
    ascending: (a: Row, b: Row) => Number(a.waiting) - Number(b.waiting),
    first: 'desc',
  },
} satisfies Record<
  string,
  { label: string; ascending: (a: Row, b: Row) => number; first: Direction }
>;
type Column = keyof typeof columns;
const byName: { column: Column; direction: Direction } = { column: 'name', direction: 'asc' };

/**
 * Everyone who stands at a tier in some category, or has a writeup awaiting signature: their
 * tier in each category, the highest first. The list opens by name and sorts by the column
 * clicked, back to by name each time the tab opens. On a phone their totals sit under their
 * name, where the columns for them don't fit, and the list stays by name.
 */
export function DriversTab({
  overview,
  openDriver,
  filters,
  change,
}: {
  overview: ConductOverview;
  openDriver: (code: string) => void;
  filters: HistoryFilters;
  change: (filters: HistoryFilters) => void;
}) {
  const { search, category, tier, controls } = useConductFilters(
    overview.categories,
    filters,
    change,
  );
  const [sort, setSort] = useState(byName);
  const rows = useMemo(() => {
    const index = indexOf(overview);
    const turn = sort.direction === 'asc' ? 1 : -1;
    return overview.drivers
      .map((driver) => {
        const standings = index.standings.get(driver.person) ?? [];
        return {
          person: personOf(overview, driver.person),
          standings,
          count: driver.count,
          last: driver.latestOn,
          waiting: driver.awaitingSignature > 0,
        };
      })
      .filter((row) => row.standings.length || row.waiting)
      .filter((row) => {
        const named = row.person.name
          .toLocaleLowerCase()
          .includes(search.trim().toLocaleLowerCase());
        const held =
          (!category && !tier) ||
          row.standings.some(
            (s) => (!category || s.categoryId === category) && (!tier || s.tier === tier),
          );
        return named && held;
      })
      .sort((a, b) => turn * columns[sort.column].ascending(a, b) || columns.name.ascending(a, b));
  }, [overview, search, category, tier, sort]);
  const heading = (column: Column) => {
    const direction = sort.column === column ? sort.direction : undefined;
    return (
      <SortHeader
        direction={direction}
        indicator={
          direction === 'asc' ? (
            <ArrowUp size={14} />
          ) : direction === 'desc' ? (
            <ArrowDown size={14} />
          ) : (
            <ArrowUpDown size={14} />
          )
        }
        onSort={() =>
          setSort({
            column,
            direction: direction ? (direction === 'asc' ? 'desc' : 'asc') : columns[column].first,
          })
        }
      >
        {columns[column].label}
      </SortHeader>
    );
  };
  useListScroll(true);
  return (
    <>
      <div className="table-toolbar conduct-toolbar">{controls}</div>
      {rows.length === 0 ? (
        <Empty title="No drivers" />
      ) : (
        <div className="table-wrap">
          <table className="conduct-table conduct-stacked conduct-drivers">
            <thead>
              <tr>
                {heading('name')}
                <th>Current tier</th>
                {heading('count')}
                {heading('last')}
                {heading('waiting')}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.person.code}
                  className="conduct-row"
                  onClick={() => openDriver(row.person.code)}
                >
                  <td>
                    <RowLink open={() => openDriver(row.person.code)}>
                      <Person person={row.person} />
                    </RowLink>
                    <div className="conduct-row-details">
                      {row.count} {row.count === 1 ? 'writeup' : 'writeups'}
                      {row.last && ` · Last ${printedDay(row.last)}`}
                    </div>
                  </td>
                  <td>
                    <div className="conduct-chips">
                      {row.standings.map((s) => (
                        <TierBadge
                          key={s.categoryId}
                          tier={s.tier}
                          category={categoryName(overview, s.categoryId)}
                        />
                      ))}
                    </div>
                  </td>
                  <td>{row.count}</td>
                  <td className="conduct-nowrap">{row.last && printedDay(row.last)}</td>
                  <td>
                    {row.waiting && (
                      <span className="conduct-state conduct-state-waiting">
                        Awaiting signature
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
