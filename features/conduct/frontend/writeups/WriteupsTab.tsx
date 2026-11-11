import { useUpdateState } from '../../../../core/shell/frontend/runtime/navigation-state.js';
import { PenLine } from 'lucide-react';
import { DataState, Empty } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview, ConductWriteupRow } from '../../api/index.js';
import { Person, RowLink, SignedState, WriteupTier, categoryName, personOf } from '../parts.js';
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
  person,
  history = false,
}: {
  overview: ConductOverview;
  openWriteup: (id: string) => void;
  markSigned?: (writeup: ConductWriteupRow) => void;
  filters: HistoryFilters;
  change: (filters: HistoryFilters) => void;
  person?: string;
  history?: boolean;
}) {
  const { controls } = useConductFilters(overview.categories, filters, change);
  return (
    <>
      {!history && (
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
      )}
      <HistoryRows
        key={JSON.stringify([person, filters])}
        overview={overview}
        filters={filters}
        person={person}
        openWriteup={openWriteup}
        markSigned={markSigned}
      />
    </>
  );
}

function HistoryRows({
  overview,
  filters,
  person,
  openWriteup,
  markSigned,
}: Omit<Parameters<typeof WriteupsTab>[0], 'change' | 'history'>) {
  const [navigation, setNavigation] = useUpdateState(
    `conduct-history:${JSON.stringify([person, filters])}`,
    { page: 0, cursors: [''] },
  );
  const read = useConductHistory(filters, person, navigation.cursors[navigation.page]);
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
                        {w.signedOn || !markSigned ? (
                          <SignedState writeup={w} />
                        ) : (
                          <button
                            type="button"
                            className="conduct-small"
                            disabled={!markSigned}
                            onClick={(event) => {
                              event.stopPropagation();
                              markSigned?.(w);
                            }}
                          >
                            <PenLine size={14} />
                            Mark signed
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="form-actions">
            <button
              type="button"
              disabled={navigation.page === 0}
              onClick={() => setNavigation((n) => ({ ...n, page: n.page - 1 }))}
            >
              Previous
            </button>
            <button
              type="button"
              disabled={!read.data?.nextCursor}
              onClick={() => {
                const next = read.data?.nextCursor;
                if (next)
                  setNavigation((n) => ({
                    page: n.page + 1,
                    cursors: [...n.cursors.slice(0, n.page + 1), next],
                  }));
              }}
            >
              Next
            </button>
          </div>
        </>
      )}
    </DataState>
  );
}
