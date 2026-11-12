import { useUpdateState } from '../../../../core/shell/frontend/runtime/navigation-state.js';
import { DataState, Empty } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview, ConductWriteupRow } from '../../api/index.js';
import { Person, RowLink, WriteupStatus, WriteupTier, categoryName, personOf } from '../parts.js';
import { printedDay } from '../tiers.js';
import { useConductHistory, type HistoryFilters } from '../../api/client.js';
import { useListScroll } from '../useListScroll.js';
import { useConductFilters } from '../useConductFilters.js';

/** Every writeup, the newest first, with those awaiting signature marked signed in the row. */
export function WriteupsTab({
  overview,
  openWriteup,
  markSigned,
  filters,
  change,
}: {
  overview: ConductOverview;
  openWriteup: (id: string) => void;
  markSigned: (writeup: ConductWriteupRow) => void;
  filters: HistoryFilters;
  change: (filters: HistoryFilters) => void;
}) {
  const { controls } = useConductFilters(overview.categories, filters, change);
  return (
    <>
      <div className="table-toolbar conduct-toolbar">
        {controls}
        <select
          aria-label="Status"
          value={filters.status}
          onChange={(e) => change({ ...filters, status: e.target.value })}
        >
          <option value="">All statuses</option>
          <option value="waiting">Awaiting signature</option>
          <option value="signed">Signed</option>
        </select>
        <button
          type="button"
          aria-pressed={filters.status === 'waiting'}
          onClick={() =>
            change({ ...filters, status: filters.status === 'waiting' ? '' : 'waiting' })
          }
        >
          Awaiting signature
        </button>
      </div>
      <HistoryRows
        key={JSON.stringify(filters)}
        overview={overview}
        filters={filters}
        openWriteup={openWriteup}
        markSigned={markSigned}
      />
    </>
  );
}

function HistoryRows({
  overview,
  filters,
  openWriteup,
  markSigned,
}: Omit<Parameters<typeof WriteupsTab>[0], 'change'>) {
  const { read, ...pages } = useHistoryPages(filters);
  useListScroll(Boolean(read.data));
  const rows = read.data?.writeups ?? [];
  return (
    <DataState data={read.data} error={read.error} retry={read.refresh}>
      {() => (
        <>
          {rows.length === 0 ? (
            <Empty title="No writeups" />
          ) : (
            <div className="table-wrap">
              <table className="conduct-table">
                <thead>
                  <tr>
                    <th>Date</th>
                    <th>Driver</th>
                    <th>Category</th>
                    <th>Tier</th>
                    <th>Supervisor</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((w) => (
                    <tr key={w.id} className="conduct-row" onClick={() => openWriteup(w.id)}>
                      <td className="conduct-nowrap">
                        <RowLink open={() => openWriteup(w.id)}>{printedDay(w.writtenOn)}</RowLink>
                      </td>
                      <td>
                        <Person person={personOf(overview, w.person)} />
                      </td>
                      <td>{categoryName(overview, w.categoryId)}</td>
                      <td>
                        <WriteupTier writeup={w} />
                      </td>
                      <td className="conduct-nowrap">{w.supervisorName}</td>
                      <td>
                        <WriteupStatus writeup={w} markSigned={markSigned} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <Pager {...pages} />
        </>
      )}
    </DataState>
  );
}

/** One page of the writeups `filters` and `person` name at a time, which page in the address. */
export function useHistoryPages(filters: HistoryFilters, person?: string) {
  const [navigation, setNavigation] = useUpdateState(
    `conduct-history:${JSON.stringify([person, filters])}`,
    { page: 0, cursors: [''] },
  );
  const read = useConductHistory(filters, person, navigation.cursors[navigation.page]);
  const next = read.data?.nextCursor;
  return {
    read,
    previous:
      navigation.page > 0 ? () => setNavigation((n) => ({ ...n, page: n.page - 1 })) : undefined,
    next: next
      ? () =>
          setNavigation((n) => ({
            page: n.page + 1,
            cursors: [...n.cursors.slice(0, n.page + 1), next],
          }))
      : undefined,
  };
}

/** Going to the page before or after, either disabled where there's none. */
export function Pager({ previous, next }: { previous?: () => void; next?: () => void }) {
  return (
    <div className="form-actions">
      <button type="button" disabled={!previous} onClick={previous}>
        Previous
      </button>
      <button type="button" disabled={!next} onClick={next}>
        Next
      </button>
    </div>
  );
}
