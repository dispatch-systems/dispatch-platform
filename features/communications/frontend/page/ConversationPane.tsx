import { useEffect } from 'react';
import { ArrowLeft, Check } from 'lucide-react';
import type { Conversation, Message, Writing } from '../../api/index.js';
import { markDone } from '../../api/client.js';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Composer } from './Composer.js';
import { Face } from './Face.js';
import { roles, Wait } from './parts.js';
import { Thread } from './Thread.js';
import { useConversation } from './useConversation.js';
import type { Changed } from './useInbox.js';

// The conversation open: who it's with and, while it waits, how long and Done; its messages;
// and writing in it. On a phone it takes the page, with a way back to the list.

export function ConversationPane({
  id,
  listed,
  changed,
  typing,
  timeZone,
  now,
  show,
  back,
  closed,
}: {
  id: string;
  /** The conversation as the list has it, until it's read. */
  listed?: Conversation;
  changed: Changed;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  /** Shows the conversation in the list as the page's own writes left it. */
  show: (conversation: Conversation) => void;
  back: () => void;
  /** Leaves a conversation that's no longer there. */
  closed: () => void;
}) {
  const { opened, error, gone, readOlder, ...read } = useConversation(id, changed);
  useEffect(() => {
    if (gone) closed();
  }, [gone, closed]);
  const conversation = opened?.conversation ?? listed;
  const done = useAction(async () => {
    const after = await markDone(id);
    read.show(after);
    show(after);
  });
  const sent = (message: Message) => {
    if (!conversation) return;
    // Answered, it waits no more; the next update says so for everyone.
    const after: Conversation = {
      ...conversation,
      state: 'responded',
      waitingSince: null,
      lastAt: message.at,
      last: message,
    };
    read.show(after, message);
    show(after);
  };
  const writing = typing.filter((writer) => writer.conversationId === id);
  const person = conversation?.with;
  return (
    <section className="pane-thread" aria-label={person?.name ?? 'Conversation'}>
      <header className="thread-head">
        <button className="icon-button back" aria-label="Back" onClick={back}>
          <ArrowLeft size={18} />
        </button>
        {person && conversation && (
          <>
            <div className="who">
              <Face {...person} size={40} />
              <div>
                <strong>{person.name}</strong>
                {(person.role || conversation.waitingSince !== null) && (
                  <small>
                    {person.role && roles[person.role]}
                    {person.role && conversation.waitingSince !== null && ' · '}
                    {conversation.waitingSince !== null && (
                      <Wait since={conversation.waitingSince} now={now} />
                    )}
                  </small>
                )}
              </div>
            </div>
            {conversation.state === 'waiting' && (
              <div className="actions">
                <button disabled={done.busy} onClick={() => void done.run()}>
                  <Check size={15} />
                  Done
                </button>
              </div>
            )}
          </>
        )}
      </header>
      <ErrorBox message={error} />
      {opened && conversation ? (
        <Thread
          conversation={conversation}
          messages={opened.messages}
          more={opened.more}
          readOlder={readOlder}
          writing={writing}
          timeZone={timeZone}
          now={now}
        />
      ) : (
        <div className="thread">{!error && <Loading />}</div>
      )}
      {conversation && <Composer conversation={conversation} sent={sent} />}
    </section>
  );
}
