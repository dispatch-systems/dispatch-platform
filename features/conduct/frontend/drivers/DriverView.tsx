import { ChevronLeft, Plus } from 'lucide-react';
import { DataState, Empty } from '../../../../core/shell/frontend/ui/index.js';
import type { ConductOverview, ConductStanding, ConductWriteupRow } from '../../api/index.js';
import { emptyFilters } from '../../api/client.js';
import { indexOf } from '../data.js';
import {
  Avatar,
  RowLink,
  TierBadge,
  WriteupStatus,
  WriteupTier,
  categoryName,
  personOf,
} from '../parts.js';
import { printedDay, stepDown, tierShort, tiers } from '../tiers.js';
import { useListScroll } from '../useListScroll.js';
import { Pager, useHistoryPages } from '../writeups/WriteupsTab.js';
import { useMarkSigned } from '../writeups/useMarkSigned.js';

/** One driver: their totals beside one card for each category they've had a writeup in, where
 * they stand there, on its ladder, when they next step down, and its writeups. The categories
 * they stand at a tier in come first, the highest first. */
export function DriverView({
  overview,
  code,
  back,
  write,
  openWriteup,
  take,
}: {
  overview: ConductOverview;
  code: string;
  back: () => void;
  write: () => void;
  openWriteup: (id: string) => void;
  take: () => void;
}) {
  const person = personOf(overview, code);
  const standings = indexOf(overview).standings.get(code) ?? [];
  const driver = overview.drivers.find((d) => d.person === code);
  const held = new Map(standings.map((s) => [s.categoryId, s]));
  const categories = [...held.keys(), ...(driver?.categories ?? []).filter((id) => !held.has(id))];
  const { markSigned, dialog } = useMarkSigned(overview, take);
  return (
    <>
      <button type="button" className="conduct-back" onClick={back}>
        <ChevronLeft size={15} />
        Conduct
      </button>
      <div className="conduct-driver">
        <aside className="conduct-driver-card">
          <Avatar person={person} className="conduct-avatar conduct-avatar-large" />
          <h1>{person.name}</h1>
          <button type="button" className="primary" onClick={write}>
            <Plus size={16} />
            New writeup
          </button>
          {driver && (
            <dl className="detail-list">
              {standings[0] && (
                <div>
                  <dt>Highest tier</dt>
                  <dd>
                    <TierBadge tier={standings[0].tier} />
                  </dd>
                </div>
              )}
              <div>
                <dt>Writeups</dt>
                <dd>{driver.count}</dd>
              </div>
              <div>
                <dt>Awaiting signature</dt>
                <dd className={driver.awaitingSignature ? 'conduct-awaiting' : undefined}>
                  {driver.awaitingSignature}
                </dd>
              </div>
              <div>
                <dt>Last writeup</dt>
                <dd>{printedDay(driver.latestOn)}</dd>
              </div>
            </dl>
          )}
        </aside>
        <div className="conduct-categories">
          {categories.length === 0 ? (
            <Empty title="No writeups" />
          ) : (
            categories.map((id) => (
              <CategoryHistory
                key={id}
                overview={overview}
                person={code}
                category={id}
                standing={held.get(id)}
                openWriteup={openWriteup}
                markSigned={markSigned}
              />
            ))
          )}
        </div>
      </div>
      {dialog}
    </>
  );
}

/** A category's card: where the driver stands in it, and their writeups in it, newest first. */
function CategoryHistory({
  overview,
  person,
  category,
  standing,
  openWriteup,
  markSigned,
}: {
  overview: ConductOverview;
  person: string;
  category: string;
  standing?: ConductStanding;
  openWriteup: (id: string) => void;
  markSigned: (writeup: ConductWriteupRow) => void;
}) {
  const { read, ...pages } = useHistoryPages({ ...emptyFilters, category }, person);
  // Each card restores the list's scroll once its rows exist, so the last to arrive settles it.
  useListScroll(Boolean(read.data));
  const next = standing && stepDown(standing);
  return (
    <section className={`conduct-category conduct-category-${standing?.tier ?? 'clean'}`}>
      <header>
        <div>
          <h2>
            {categoryName(overview, category)}
            {standing && <TierBadge tier={standing.tier} />}
          </h2>
          {next && <p>{next}</p>}
        </div>
        <TierSteps standing={standing} />
      </header>
      <DataState data={read.data} error={read.error} retry={read.refresh}>
        {({ writeups }) => (
          <div className="table-wrap">
            <table className="conduct-table">
              <thead>
                <tr>
                  <th>Date</th>
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
      </DataState>
      {(pages.previous || pages.next) && <Pager {...pages} />}
    </section>
  );
}

/** Verbal, Written and Final, those up to where the driver stands filled in. */
function TierSteps({ standing }: { standing?: ConductStanding }) {
  const reached = standing ? tiers.indexOf(standing.tier) : -1;
  return (
    <ol className="conduct-steps" aria-hidden="true">
      {tiers.map((tier, at) => (
        <li
          key={tier}
          className={
            at < reached ? 'reached' : at === reached ? 'reached conduct-step-current' : undefined
          }
        >
          {tierShort[tier]}
        </li>
      ))}
    </ol>
  );
}
