import { ChevronLeft, Plus } from 'lucide-react';
import type { ConductOverview } from '../../api/index.js';
import { Person, TierBadge, categoryName, personOf } from '../parts.js';
import { emptyFilters } from '../../api/client.js';
import { WriteupsTab } from '../writeups/WriteupsTab.js';
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
            const note =
              standing.tier === 'final'
                ? finalNote(standing.latestFinalKind, standing.latestSuspension)
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
      <WriteupsTab
        overview={overview}
        filters={emptyFilters}
        change={() => {}}
        person={code}
        history
        openWriteup={openWriteup}
      />
    </>
  );
}
