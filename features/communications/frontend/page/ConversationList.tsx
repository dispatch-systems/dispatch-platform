import { useCallback } from 'react';
import { Check } from 'lucide-react';
import type { Communications, Conversation, Writing } from '../../api/index.js';
import { searchConversations } from '../../api/client.js';
import { listTime, preview } from '../format.js';
import { Face } from './Face.js';
import { More } from './More.js';
import { Name, Wait, typingNote } from './parts.js';
import { useFound } from './useFound.js';

// Messages: the conversations, of every workspace or the one chosen, those waiting on an answer,
// then those answered; or those found by a name typed, misspelt or not. The conversation open is
// at the top while the list doesn't hold it, as one New message opened before it holds a message.

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
  team,
  typed,
  pinned,
  readOlder,
  ...listing
}: Listing & {
  inbox: Communications;
  /** The workspace chosen, if one is. */
  team?: string;
  /** What's typed in Search, trimmed. */
  typed: string;
  /** The conversation open, while the list doesn't hold it. */
  pinned?: Conversation;
  readOlder: () => void;
}) {
  const search = useCallback(
    () => searchConversations(typed, team).then((found) => found.conversations),
    [typed, team],
  );
  const { found, failed } = useFound(typed ? search : undefined, inbox.revision, 200);
  const rows = (conversations: readonly Conversation[]) => (
    <ul className="convos">
      {conversations.map((conversation) => (
        <Row key={conversation.id} conversation={conversation} {...listing} />
      ))}
    </ul>
  );
  if (typed) {
    if (failed || found?.length === 0)
      return <p className="list-empty">{failed || 'No matches'}</p>;
    return found ? rows(found) : null;
  }
  const shown = inbox.conversations.filter((conversation) => !team || conversation.teamId === team);
  const waiting = shown.filter((conversation) => conversation.state === 'waiting');
  const answered = shown.filter((conversation) => conversation.state !== 'waiting');
  return (
    <>
      {pinned &&
        (!team || pinned.teamId === team) &&
        (pinned.lastAt === null ? (
          <ul className="convos">
            <li className="convo selected">
              <button className="convo-open" onClick={() => listing.select(pinned.id)}>
                <Face {...pinned.with} size="list" />
                <span className="convo-body">
                  <span className="line1">
                    <strong>{pinned.with.name}</strong>
                  </span>
                  <span className="line2">
                    <span className="preview">New message</span>
                  </span>
                </span>
              </button>
            </li>
          </ul>
        ) : (
          rows([pinned])
        ))}
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
        <Face {...person} size="list" />
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
                    size="sender"
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
