import type { ReactNode, Ref } from 'react';
import { ArrowLeft, Check } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
import { Party } from './Face.js';
import { Wait, type OpensProfile } from './parts.js';

// A conversation's head: the way back to the list, on a phone; who it's with, what they do and
// where, or their number, and while it waits how long, which opens their profile where they have
// one; what else can be done with it; and while it waits, Done, on a phone its mark alone.

export function ConversationHead({
  conversation,
  about,
  now,
  who,
  open,
  opens,
  back,
  done,
  actions,
}: {
  conversation?: Conversation;
  /** What they do for the DSP, and where; or their route today. */
  about: ReactNode;
  now: number;
  /** Who it's with, as a button, which focus goes back to as the profile closes. */
  who?: Ref<HTMLButtonElement>;
  /** Whether their profile is open. */
  open: boolean;
  /** What opens their profile; none where there's no profile to open. */
  opens?: Partial<OpensProfile>;
  back: () => void;
  done: { busy: boolean; run: () => void };
  /** What else can be done with it, before Done. */
  actions?: ReactNode;
}) {
  const person = conversation?.with;
  const waits = conversation?.state === 'waiting';
  const shown = person && conversation && (
    <>
      <Party conversation={conversation} size="head" />
      <span>
        <strong>{person.name}</strong>
        {(about || conversation.waitingSince !== null) && (
          <small>
            {about && (
              <span className="about">
                {about}
                {conversation.waitingSince !== null && ' ·'}
              </span>
            )}
            {conversation.waitingSince !== null && (
              <Wait since={conversation.waitingSince} now={now} />
            )}
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
