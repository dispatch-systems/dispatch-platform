import { useMemo, useState } from 'react';
import { PenLine } from 'lucide-react';
import { Empty } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview, ConductWriteupRow } from '../../api/index.js';
import { Person, RowLink, SignedState, WriteupTier, categoryName, personOf } from '../parts.js';
import { printedDay } from '../tiers.js';
import { useConductFilters } from '../useConductFilters.js';

type Status = '' | 'waiting' | 'signed';

/** Every writeup, the newest first, with those awaiting signature marked signed in the row. */
export function WriteupsTab({
  overview,
  openWriteup,
  markSigned,
}: {
  overview: ConductOverview;
  openWriteup: (id: string) => void;
  markSigned: (writeup: ConductWriteupRow) => void;
}) {
  const { search, category, tier, controls } = useConductFilters(overview.categories);
  const [status, setStatus] = useState<Status>('');
  const rows = useMemo(() => {
    const words = search.trim().toLocaleLowerCase();
    return overview.writeups.filter(
      (w) =>
        personOf(overview, w.person).name.toLocaleLowerCase().includes(words) &&
        (!category || w.categoryId === category) &&
        (!tier || w.tier === tier) &&
        (!status || (status === 'signed') === Boolean(w.signedOn)),
    );
  }, [overview, search, category, tier, status]);
  return (
    <>
      <div className="table-toolbar conduct-toolbar">
        {controls}
        <select
          aria-label="Status"
          value={status}
          onChange={(e) => setStatus(e.target.value as Status)}
        >
          <option value="">All statuses</option>
          <option value="waiting">Awaiting signature</option>
          <option value="signed">Signed</option>
        </select>
      </div>
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
                    {w.signedOn ? (
                      <SignedState writeup={w} />
                    ) : (
                      <button
                        type="button"
                        className="conduct-small"
                        onClick={(event) => {
                          event.stopPropagation();
                          markSigned(w);
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
    </>
  );
}
