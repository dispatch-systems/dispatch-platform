import { Fingerprint } from 'lucide-react';
import type { DriverMatch } from '../../../shared/contracts/index.js';

/** What Driver Match does, and how the DSP's people stand. */
export function DriverSummary({ data }: { data: DriverMatch }) {
  const { counts } = data;
  const stats: [string, number, string][] = [
    ['', counts.drivers, 'Drivers'],
    ['matched', counts.matched, 'Matched everywhere'],
    ['review', counts.review, counts.review === 1 ? 'Pair to review' : 'Pairs to review'],
    ['office', counts.office, 'Office staff'],
  ];
  return (
    <section className="driver-summary" aria-labelledby="driver-summary-title">
      <div className="driver-summary-copy">
        <Fingerprint size={20} aria-hidden="true" />
        <h2 id="driver-summary-title">One code per driver</h2>
      </div>
      <div className="driver-stats">
        {stats.map(([tone, value, label]) => (
          <div key={label} className={`driver-stat ${tone}`}>
            <strong>{value.toLocaleString('en-US')}</strong>
            <span>
              <i />
              {label}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
