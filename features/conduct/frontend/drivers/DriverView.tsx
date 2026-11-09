import { ChevronLeft, Plus } from 'lucide-react';
import { Empty } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview } from '../../api/index.js';
import {
  Person,
  RowLink,
  SignedState,
  TierBadge,
  WriteupTier,
  categoryName,
  personOf,
} from '../parts.js';
import { finalNote, printedDay, stepDown, tiers } from '../tiers.js';

/** One driver: where they stand in each category, on its ladder, when they next step down, and
 * every writeup they've had. */
export function DriverView({
  overview,
  code,
  back,
  write,
  openWriteup,
}: {
  overview: ConductOverview;
  code: string;
  back: () => void;
  write: () => void;
  openWriteup: (id: string) => void;
}) {
  const person = personOf(overview, code);
  const standings = overview.standings.filter((s) => s.person === code);
  const writeups = overview.writeups.filter((w) => w.person === code);
  return (
    <>
      <button type="button" className="conduct-back" onClick={back}>
        <ChevronLeft size={15} />
        Conduct
      </button>
      <div className="page-heading">
        <h1 className="conduct-driver-name">
          <Person person={person} large />
        </h1>
        <div className="heading-actions">
          <button type="button" className="primary" onClick={write}>
            <Plus size={16} />
            New writeup
          </button>
        </div>
      </div>
      {standings.length > 0 && (
        <div className="conduct-ladders">
          {standings.map((standing) => {
            const latest = writeups.find((w) => w.id === standing.latestId);
            const note =
              latest?.tier === 'final' && standing.tier === 'final'
                ? finalNote(latest.finalKind, latest.suspension)
                : '';
            const next = stepDown(standing);
            const reached = tiers.indexOf(standing.tier);
            return (
              <section key={standing.categoryId} className="conduct-ladder-card">
                <h3>
                  {categoryName(overview, standing.categoryId)}
                  <TierBadge tier={standing.tier} />
                </h3>
                <div
                  className={`conduct-ladder conduct-ladder-${standing.tier}`}
                  aria-hidden="true"
                >
                  {tiers.map((tier, at) => (
                    <span key={tier} className={at <= reached ? 'reached' : undefined} />
                  ))}
                </div>
                <div className="conduct-ladder-labels" aria-hidden="true">
                  <span>Verbal</span>
                  <span>Written</span>
                  <span>Final</span>
                </div>
                <p className="conduct-ladder-when">
                  Last writeup {printedDay(standing.latestOn)}
                  {note && ` · ${note}`}
                  {next && (
                    <>
                      <br />
                      {next}
                    </>
                  )}
                </p>
              </section>
            );
          })}
        </div>
      )}
      <h2 className="conduct-section-title">History</h2>
      {writeups.length === 0 ? (
        <Empty title="No writeups" />
      ) : (
        <div className="table-wrap">
          <table className="conduct-table">
            <thead>
              <tr>
                <th>Date</th>
                <th>Category</th>
                <th>Tier</th>
                <th>Supervisor</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {writeups.map((w) => (
                <tr key={w.id} className="conduct-row" onClick={() => openWriteup(w.id)}>
                  <td className="conduct-nowrap">
                    <RowLink open={() => openWriteup(w.id)}>{printedDay(w.writtenOn)}</RowLink>
                  </td>
                  <td>{categoryName(overview, w.categoryId)}</td>
                  <td>
                    <WriteupTier writeup={w} />
                  </td>
                  <td className="conduct-nowrap">{w.supervisorName}</td>
                  <td>
                    <SignedState writeup={w} />
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
