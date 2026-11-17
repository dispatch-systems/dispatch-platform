import type { ConductWriteup } from '../../api/index.js';
import { WriteupTier } from '../parts.js';
import { printedDay } from '../tiers.js';

/** A writeup's own words as text, for a phone, where the page it prints on is too small to read. */
export function WriteupText({ writeup }: { writeup: ConductWriteup }) {
  return (
    <div className="conduct-writeup-text">
      <dl className="detail-list">
        <div>
          <dt>Date</dt>
          <dd>{printedDay(writeup.writtenOn)}</dd>
        </div>
        <div>
          <dt>Tier</dt>
          <dd>
            <WriteupTier writeup={writeup} />
          </dd>
        </div>
        <div>
          <dt>Supervisor</dt>
          <dd>{writeup.supervisorName}</dd>
        </div>
      </dl>
      {(
        [
          ['1. Statement of the problem', writeup.statement],
          ['2. Prior discussion or warnings', writeup.prior],
          ['3. Corrective action', writeup.plan],
        ] as const
      ).map(([label, text]) => (
        <section key={label}>
          <h2 className="conduct-section-title">{label}</h2>
          <p>{text}</p>
        </section>
      ))}
    </div>
  );
}
