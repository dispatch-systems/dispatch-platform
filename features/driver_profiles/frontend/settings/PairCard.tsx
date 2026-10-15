import { ArrowRight, Check, ChevronDown, CircleAlert, CircleCheck, GitMerge } from 'lucide-react';
import { useState } from 'react';
import type { Driver, DriverPair } from '../../api/index.js';
import { keepDriversApart, mergeDrivers } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { evidenceSupports, evidenceText, shortId } from '../driver-match.js';
import { DriverAvatar } from '../parts/DriverAvatar.js';
import { SourceMark } from '../parts/Tags.js';
import { MergeDialog } from '../person/MergeDialog.js';
import { Comparison } from './Comparison.js';

/** One side of a pair: who they are, and the sources that know them. */
function Side({ driver }: { driver: Driver }) {
  const first = driver.ids[0];
  return (
    <span className="driver-pair-side">
      <DriverAvatar driver={driver} />
      <span>
        <b>{driver.name}</b>
        <small>
          {driver.ids.map((id) => (
            <SourceMark key={`${id.source}:${id.id}`} source={id.source} small />
          ))}
          {driver.ids.length === 1 && first ? shortId(first) : driver.code}
        </small>
      </span>
    </span>
  );
}

/**
 * Two people who may be one, why, and the decision: the same person, kept as one with the
 * candidate's code, or not them, never suggested again. Opening it lines their details up.
 */
export function PairCard({ pair, drivers }: { pair: DriverPair; drivers: Driver[] }) {
  const [open, setOpen] = useState(false);
  const [elsewhere, setElsewhere] = useState(false);
  const names = `${pair.candidate.name} and ${pair.person.name}`;
  const merge = useAction(() => mergeDrivers(pair.person.code, pair.candidate.code), {
    success: `${names} are now one person`,
  });
  const apart = useAction(() => keepDriversApart(pair.person.code, pair.candidate.code), {
    success: `${names} are kept as different people`,
  });
  const busy = merge.busy || apart.busy;
  const strong = pair.strength === 'strong';
  return (
    <div className="driver-pair">
      <button
        type="button"
        className="driver-pair-sides"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Side driver={pair.person} />
        <ArrowRight size={15} aria-hidden="true" />
        <Side driver={pair.candidate} />
        <ChevronDown size={16} aria-hidden="true" className="driver-pair-open" />
      </button>
      <div className="driver-pair-row">
        <span className={`driver-tag ${strong ? 'sage' : 'warning'}`}>
          {strong ? (
            <CircleCheck size={12} aria-hidden="true" />
          ) : (
            <CircleAlert size={12} aria-hidden="true" />
          )}
          {strong ? 'Strong match' : 'Possible match'}
        </span>
        <ul className="driver-evidence" aria-label="Why they may be one person">
          {pair.evidence.map((evidence, index) => (
            <li
              key={`${evidence.kind}:${index}`}
              className={evidenceSupports(evidence) ? undefined : 'against'}
            >
              {evidenceSupports(evidence) ? (
                <Check size={12} aria-hidden="true" />
              ) : (
                <CircleAlert size={12} aria-hidden="true" />
              )}
              {evidenceText(evidence)}
            </li>
          ))}
        </ul>
        <div className="driver-pair-actions">
          <button disabled={busy} onClick={() => void apart.run()}>
            Not them
          </button>
          <button className="primary" disabled={busy} onClick={() => void merge.run()}>
            <GitMerge size={15} aria-hidden="true" />
            Same person
          </button>
        </div>
      </div>
      {open && (
        <>
          <Comparison pair={pair} />
          <div className="driver-pair-more">
            <button onClick={() => setElsewhere(true)}>Someone else…</button>
          </div>
        </>
      )}
      {elsewhere && (
        <MergeDialog driver={pair.person} drivers={drivers} onClose={() => setElsewhere(false)} />
      )}
    </div>
  );
}
