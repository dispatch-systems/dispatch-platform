import { useMemo } from 'react';
import { Empty } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview } from '../../api/index.js';
import { Person, RowLink, TierBadge, categoryName, personOf } from '../parts.js';
import { printedDay, tiers } from '../tiers.js';
import { indexOf } from '../data.js';
import { type HistoryFilters } from '../../api/client.js';
import { useListScroll } from '../useListScroll.js';
import { useConductFilters } from '../useConductFilters.js';

/**
 * Everyone who stands at a tier in some category, or has a writeup awaiting signature: their
 * tier in each category, the highest first, and the people with the highest tiers first.
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
  const rows = useMemo(() => {
    const index = indexOf(overview);
    return overview.drivers
      .map((driver) => {
        const standings = index.standings.get(driver.person) ?? [];
        return {
          person: personOf(overview, driver.person),
          standings,
          highest: standings[0] ? tiers.indexOf(standings[0].tier) : -1,
          count: driver.count,
          last: driver.latestOn,
          waiting: driver.awaitingSignature,
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
      .sort((a, b) => b.highest - a.highest || b.last.localeCompare(a.last));
  }, [overview, search, category, tier]);
  useListScroll(true);
  return (
    <>
      <div className="table-toolbar conduct-toolbar">{controls}</div>
      {rows.length === 0 ? (
        <Empty title="No drivers" />
      ) : (
        <div className="table-wrap">
          <table className="conduct-table">
            <thead>
              <tr>
                <th>Driver</th>
                <th>Current tier</th>
                <th>Writeups</th>
                <th>Last writeup</th>
                <th>
                  <span className="sr-only">Signature</span>
                </th>
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
