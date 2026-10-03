import { CircleAlert } from 'lucide-react';
import type { DriverPair } from '../../../shared/contracts/driver-match.js';
import { ReviewPair } from './ReviewPair.js';

/** The pairs waiting for a decision, above everyone else. */
export function ReviewCard({
  pairs,
  onOpen,
}: {
  pairs: DriverPair[];
  onOpen: (code: string) => void;
}) {
  const count = pairs.length;
  return (
    <section className="driver-review" aria-labelledby="driver-review-title">
      <div className="driver-review-head">
        <CircleAlert size={18} aria-hidden="true" />
        <h2 id="driver-review-title">
          {count === 1
            ? '1 driver might be listed twice'
            : `${count} drivers might be listed twice`}
        </h2>
      </div>
      {pairs.map((pair) => (
        <ReviewPair key={`${pair.paycom.code}:${pair.amazon.code}`} pair={pair} onOpen={onOpen} />
      ))}
    </section>
  );
}
