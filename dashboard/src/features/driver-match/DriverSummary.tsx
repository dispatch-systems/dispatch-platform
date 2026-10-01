import { Fingerprint } from 'lucide-react';
import type { DriverMatch } from '../../../../shared/contracts/index.js';
import { time } from '../../lib/format.js';

/** What Driver Match does, and how the DSP's people stand. */
export function DriverSummary({ data, timezone }: { data: DriverMatch; timezone: string }) {
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
        <div>
          <h2 id="driver-summary-title">One code per driver</h2>
          <p className="driver-match-copy">
            Each driver gets a code that ties together their timecards, routes, meal breaks, DVIC
            and scorecard, even when Paycom and Amazon write their name differently.
          </p>
          <small>
            Checked after every collection
            {data.checkedAt && ` · Last checked ${time(data.checkedAt, timezone)}`}
          </small>
        </div>
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
