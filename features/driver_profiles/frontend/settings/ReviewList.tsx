import { useState } from 'react';
import type { Driver, DriverPair } from '../../api/index.js';
import { PairCard } from './PairCard.js';

/** How many pairs show before the rest are asked for. */
const SHOWN = 3;

/** The pairs waiting for a decision, the strongest first. */
export function ReviewList({ pairs, drivers }: { pairs: DriverPair[]; drivers: Driver[] }) {
  const [all, setAll] = useState(false);
  if (!pairs.length) return null;
  const shown = all ? pairs : pairs.slice(0, SHOWN);
  return (
    <section className="driver-review" aria-labelledby="driver-review-title">
      <div className="driver-section-head">
        <h2 id="driver-review-title">Needs review</h2>
        <span className="driver-count warn">{pairs.length}</span>
      </div>
      {shown.map((pair) => (
        <PairCard
          key={`${pair.person.code}:${pair.candidate.code}`}
          pair={pair}
          drivers={drivers}
        />
      ))}
      {pairs.length > SHOWN && (
        <button type="button" className="text-button" onClick={() => setAll(!all)}>
          {all ? 'Show fewer' : `Show ${pairs.length - SHOWN} more`}
        </button>
      )}
    </section>
  );
}
