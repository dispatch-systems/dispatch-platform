import type { Ref } from 'react';
import { ArrowLeft, Check } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
import { Face } from './Face.js';
import { Wait, type OpensProfile } from './parts.js';

// A conversation's head: the way back to the list, on a phone; who it's with, what they do and
// where, and while it waits how long, which opens their profile; and while it waits, Done, on a
// phone its mark alone.

export function ConversationHead({
  conversation,
  about,
  now,
  who,
  open,
  opens,
  back,
  done,
}: {
  conversation?: Conversation;
  /** What they do for the DSP, and where. */
  about: string;
  now: number;
  /** Who it's with, as a button, which focus goes back to as the profile closes. */
  who?: Ref<HTMLButtonElement>;
  /** Whether their profile is open. */
  open: boolean;
  opens: Partial<OpensProfile>;
  back: () => void;
  done: { busy: boolean; run: () => void };
}) {
  const person = conversation?.with;
  return (
    <header className="thread-head">
      <button className="icon-button back" aria-label="Back" onClick={back}>
        <ArrowLeft size={18} />
      </button>
      {person && conversation && (
        <>
          <button ref={who} className={open ? 'who open' : 'who'} {...opens}>
            <Face {...person} size="head" />
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
          </button>
          {conversation.state === 'waiting' && (
            <div className="actions">
              <button disabled={done.busy} onClick={done.run} aria-label="Done">
                <Check size={15} />
                <span>Done</span>
              </button>
            </div>
          )}
        </>
      )}
    </header>
  );
}
