import { useEffect, useState } from 'react';
import { Check } from 'lucide-react';
import type { Communications, Conversation, Writing } from '../../api/index.js';
import { searchConversations } from '../../api/client.js';
import { SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { listTime, preview } from '../format.js';
import { Face } from './Face.js';
import { More } from './More.js';
import { Name, Wait, typingNote } from './parts.js';

// The list of conversations: those waiting on an answer, then those answered, or those found by
// a name typed, misspelt or not; with the conversation New message opened at the top until it
// holds a message.

type Listing = {
  open?: string;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  select: (id: string) => void;
  done: (conversation: Conversation) => void;
};

export function ConversationList({
  inbox,
  draft,
  readOlder,
  ...listing
}: Listing & {
  inbox: Communications;
  /** A conversation New message opened, not listed until it holds a message. */
  draft?: Conversation;
  readOlder: () => void;
}) {
  const [typed, setTyped] = useState('');
  const [found, setFound] = useState<Conversation[]>();
  // Found again as the conversations change, a moment after typing stops.
  useEffect(() => {
    const name = typed.trim();
    if (!name) {
      setFound(undefined);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      searchConversations(name).then(
        (result) => live && setFound(result.conversations),
        () => live && setFound([]),
      );
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [typed, inbox.revision]);
  const waiting = inbox.conversations.filter((conversation) => conversation.state === 'waiting');
  const answered = inbox.conversations.filter((conversation) => conversation.state !== 'waiting');
  const rows = (conversations: readonly Conversation[]) => (
    <ul className="convos">
      {conversations.map((conversation) => (
        <Row key={conversation.id} conversation={conversation} {...listing} />
      ))}
    </ul>
  );
  return (
    <section className="pane-list" aria-label="Conversations">
      <div className="list-tools">
        <SearchInput label="Search" placeholder="Search" value={typed} onChange={setTyped} />
      </div>
      <div className="list-scroll">
        {found ? (
          found.length ? (
            rows(found)
          ) : (
            <p className="list-empty">No matches</p>
          )
        ) : (
          <>
            {draft && (
              <ul className="convos">
                <li className="convo selected">
                  <button className="convo-open" onClick={() => listing.select(draft.id)}>
                    <Face {...draft.with} size={40} />
                    <span className="convo-body">
                      <span className="line1">
                        <strong>{draft.with.name}</strong>
                      </span>
                      <span className="line2">
                        <span className="preview">New message</span>
                      </span>
                    </span>
                  </button>
                </li>
              </ul>
            )}
            {waiting.length > 0 && (
              <>
                <h2 className="list-head">
                  Waiting<span>{waiting.length}</span>
                </h2>
                {rows(waiting)}
              </>
            )}
            {answered.length > 0 && (
              <>
                <h2 className="list-head">Responded</h2>
                {rows(answered)}
              </>
            )}
            {inbox.more && <More read={readOlder} />}
          </>
        )}
      </div>
    </section>
  );
}

/**
 * A conversation in the list: who it's with, when they last wrote, and what was written last, or
 * who at the DSP is writing in it. One waiting shows how long it has, and a check that marks it
 * done without a reply while it's pointed at.
 */
function Row({
  conversation,
  open,
  typing,
  timeZone,
  now,
  select,
  done,
}: Listing & { conversation: Conversation }) {
  const { with: person, last } = conversation;
  const waits = conversation.state === 'waiting';
  const writing = typing.filter((writer) => writer.conversationId === conversation.id);
  const selected = open === conversation.id;
  return (
    <li className={`convo${selected ? ' selected' : ''}${waits ? ' waits' : ''}`}>
      <button
        className="convo-open"
        aria-current={selected || undefined}
        onClick={() => select(conversation.id)}
      >
        <Face {...person} size={40} />
        <span className="convo-body">
          <span className="line1">
            <strong>
              <Name name={person.name} matched={conversation.matched} />
            </strong>
            {conversation.lastAt !== null && (
              <time>{listTime(conversation.lastAt, timeZone, now)}</time>
            )}
          </span>
          <span className="line2">
            {writing.length ? (
              <span className="preview typing-note">{typingNote(writing)}</span>
            ) : (
              <>
                {last?.sender && (
                  <Face
                    name={last.sender.name}
                    code={last.sender.code}
                    photo={last.sender.photo}
                    size={18}
                  />
                )}
                <span className="preview">{last ? preview(last) : ''}</span>
              </>
            )}
            {waits && conversation.waitingSince !== null && (
              <Wait since={conversation.waitingSince} now={now} />
            )}
          </span>
        </span>
      </button>
      {waits && (
        <button className="convo-done" aria-label="Done" onClick={() => done(conversation)}>
          <Check size={16} />
        </button>
      )}
    </li>
  );
}
