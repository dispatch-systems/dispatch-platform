import { Check as Yes, CircleAlert, CircleCheck, CircleHelp, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import type { Check } from './checks.js';
import { Comparison } from './Comparison.js';

const names = ({ sides: [left, right] }: Check) => `${right.name} and ${left.name}`;

/** The questions one at a time, from the one at `start`, each answered or skipped in turn. */
export function CheckDialog({
  checks,
  start,
  onClose,
}: {
  checks: Check[];
  start: number;
  onClose: () => void;
}) {
  const [at, setAt] = useState(start);
  // An answer takes its question away before the tab is read again.
  const [answered, setAnswered] = useState<ReadonlySet<string>>(new Set());
  const left = checks.filter((check) => !answered.has(check.key));
  const answer = useAction((check: Check, same: boolean) => check.answer(same), {
    inline: true,
    success: (check, same) =>
      `${names(check)} ${same ? 'are now one person' : 'are kept as different people'}`,
  });
  useEffect(() => {
    if (!left.length) onClose();
  }, [left.length, onClose]);
  const index = Math.min(at, left.length - 1);
  const check = left[index];
  if (!check) return null;
  const decide = async (same: boolean) => {
    if (await answer.run(check, same)) setAnswered((done) => new Set(done).add(check.key));
  };
  return (
    <Modal
      title={
        <>
          Same person?
          {left.length > 1 && (
            <>
              {' '}
              <span className="driver-check-count">
                {index + 1} of {left.length}
              </span>
            </>
          )}
        </>
      }
      onClose={onClose}
    >
      <div className="driver-check" key={check.key}>
        <Comparison sides={check.sides} />
        <div className="driver-check-why">
          <span className={`driver-verdict ${check.likely ? 'likely' : 'unsure'}`}>
            {check.likely ? (
              <CircleCheck size={15} aria-hidden="true" />
            ) : (
              <CircleHelp size={15} aria-hidden="true" />
            )}
            {check.likely ? 'Likely the same' : 'Not sure'}
          </span>
          <ul className="driver-reasons">
            {check.reasons.map((reason) => (
              <li key={reason.text} className={reason.supports ? undefined : 'against'}>
                {reason.supports ? (
                  <Yes size={15} aria-hidden="true" />
                ) : (
                  <CircleAlert size={15} aria-hidden="true" />
                )}
                {reason.text}
              </li>
            ))}
          </ul>
        </div>
        <ErrorBox message={answer.error} />
        <div className="driver-check-actions">
          {left.length > 1 && (
            <button
              className="driver-check-skip"
              disabled={answer.busy}
              onClick={() => setAt((index + 1) % left.length)}
            >
              Skip
            </button>
          )}
          <button disabled={answer.busy} onClick={() => void decide(false)}>
            <X size={17} aria-hidden="true" />
            Different people
          </button>
          <button className="primary" disabled={answer.busy} onClick={() => void decide(true)}>
            <Yes size={17} aria-hidden="true" />
            Same person
          </button>
        </div>
      </div>
    </Modal>
  );
}
