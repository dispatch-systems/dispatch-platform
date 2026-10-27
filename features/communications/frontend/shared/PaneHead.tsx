import type { ReactNode, Ref } from 'react';
import { ArrowLeft, Check } from 'lucide-react';
import type { OpensProfile } from './usePanel.js';
import { Wait } from './Wait.js';
import './pane-head.css';

// The head of what's open, a conversation or a group: the way back to the list, on a phone; who
// it's with, by their face and name, what's said of them, and while it waits how long, which opens
// their profile or its people where there's one; what else can be done with it; and while it
// waits, Done, on a phone its mark alone.

export function PaneHead({
  face,
  name,
  about,
  waits,
  waitingSince,
  now,
  who,
  open,
  opens,
  back,
  done,
  actions,
}: {
  /** Whom it's with, by their face; none until it's read. */
  face?: ReactNode;
  name?: string;
  /** What's said of them: what they do and where, their number, or a driver's route today. */
  about?: ReactNode;
  /** Whether it waits on an answer, which Done marks it answered without. */
  waits: boolean;
  /** Since when it waits, as its wait shows. */
  waitingSince: number | null;
  now: number;
  /** Who it's with, as a button, which focus goes back to as the profile closes. */
  who?: Ref<HTMLButtonElement>;
  /** Whether their profile is open. */
  open: boolean;
  /** What opens their profile; none where there's no profile to open. */
  opens?: Partial<OpensProfile>;
  back: () => void;
  /** Done, while it waits. */
  done: { busy: boolean; run: () => void };
  /** What else can be done with it, before Done. */
  actions?: ReactNode;
}) {
  const shown = name !== undefined && (
    <>
      {face}
      <span>
        <strong>{name}</strong>
        {(about || waitingSince !== null) && (
          <small>
            {about && (
              <span className="about">
                {about}
                {waitingSince !== null && ' ·'}
              </span>
            )}
            {waitingSince !== null && <Wait since={waitingSince} now={now} />}
          </small>
        )}
      </span>
    </>
  );
  return (
    <header className="thread-head">
      <button className="icon-button back" aria-label="Back" onClick={back}>
        <ArrowLeft size={18} />
      </button>
      {shown &&
        (opens ? (
          <button ref={who} className={open ? 'who open' : 'who'} {...opens}>
            {shown}
          </button>
        ) : (
          <div className="who">{shown}</div>
        ))}
      {(actions || waits) && (
        <div className="actions">
          {actions}
          {waits && (
            <button disabled={done.busy} onClick={done.run} aria-label="Done">
              <Check size={15} />
              <span>Done</span>
            </button>
          )}
        </div>
      )}
    </header>
  );
}
