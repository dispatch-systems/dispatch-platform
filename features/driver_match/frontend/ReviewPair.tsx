import { Check, CircleAlert, CircleCheck, GitMerge, Link2 } from 'lucide-react';
import type { DriverPair } from '../api/index.js';
import { keepDriversApart, mergeDrivers } from '../api/client.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { dataLabels, evidenceSupports, evidenceText, firstId, shortId } from './driver-match.js';
import { DriverButton } from './DriverButton.js';

/** Someone only Paycom knows beside someone only Amazon knows, and why they may be one. */
export function ReviewPair({ pair, onOpen }: { pair: DriverPair; onOpen: (code: string) => void }) {
  const names = `${pair.paycom.name} and ${pair.amazon.name}`;
  // The Paycom side keeps its code, so a person's code follows their employment record.
  const merge = useAction(() => mergeDrivers(pair.amazon.code, pair.paycom.code), {
    success: `${names} are now one person`,
  });
  const apart = useAction(() => keepDriversApart(pair.paycom.code, pair.amazon.code), {
    success: `${names} are kept as different people`,
  });
  const busy = merge.busy || apart.busy;
  const paycom = firstId(pair.paycom, 'paycom');
  const amazon = firstId(pair.amazon, 'amazon');
  const appears = (data: DriverPair['paycom']['appears']) =>
    data.map((d) => dataLabels[d]).join(' · ') || 'No collected data yet';
  return (
    <div className="driver-pair">
      <DriverButton driver={pair.paycom} onOpen={onOpen}>
        <small>
          <b>Paycom</b> · {[paycom?.name, paycom?.id].filter(Boolean).join(' · ')}
        </small>
        <small>{appears(pair.paycom.appears)}</small>
      </DriverButton>
      <span className="driver-pair-link" aria-hidden="true">
        <Link2 size={14} />
      </span>
      <DriverButton driver={pair.amazon} onOpen={onOpen}>
        <small>
          <b>Amazon</b> · {amazon && shortId(amazon)}
        </small>
        <small>{appears(pair.amazon.appears)}</small>
      </DriverButton>
      <div className="driver-pair-actions">
        <button disabled={busy} onClick={() => apart.run()}>
          Different people
        </button>
        <button className="primary" disabled={busy} onClick={() => merge.run()}>
          <GitMerge size={15} aria-hidden="true" />
          Same person
        </button>
      </div>
      <div className="driver-evidence-row">
        {pair.strength === 'strong' ? (
          <span className="driver-tag sage">
            <CircleCheck size={13} aria-hidden="true" />
            Strong match
          </span>
        ) : (
          <span className="driver-tag warning">
            <CircleAlert size={13} aria-hidden="true" />
            Possible match
          </span>
        )}
        <ul className="driver-evidence" aria-label="Why they may be one person">
          {pair.evidence.map((evidence, index) => (
            <li
              key={`${evidence.kind}:${index}`}
              className={evidenceSupports(evidence) ? undefined : 'against'}
            >
              {evidenceSupports(evidence) ? (
                <Check size={13} aria-hidden="true" />
              ) : (
                <CircleAlert size={13} aria-hidden="true" />
              )}
              {evidenceText(evidence)}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
