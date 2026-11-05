import type { ReactNode } from 'react';
import type { Conversation, Writing } from '../../api/index.js';
import { listTime, preview, typingNote } from '../shared/format.js';
import { PartyFace } from '../conversations/party.js';
import { ListRow } from '../shared/ListRow.js';
import { Name } from '../shared/Name.js';
import { Wait } from '../shared/Wait.js';
import type { Listing } from './listing.js';
import './conversation-row.css';

/**
 * A conversation in the list: who it's with, their face marked with where it is while `channel`,
 * when they last wrote, and what was written last, under the first name of whoever at the DSP
 * wrote it, or who at the DSP is writing in it; while it waits, how long it has, and Done.
 */
export function ConversationRow({
  conversation,
  open,
  typing,
  timeZone,
  now,
  select,
  warm,
  done,
  channel,
  below,
}: Pick<Listing, 'open' | 'typing' | 'timeZone' | 'now' | 'select' | 'warm' | 'done'> & {
  conversation: Conversation;
  /** Whether its face says where it is, as in a list of every section's. */
  channel?: boolean;
  /** What else the row shows, under what was written last. */
  below?: ReactNode;
}) {
  const { with: person } = conversation;
  const waits = conversation.state === 'waiting';
  return (
    <ListRow
      selected={open === conversation.id}
      waits={waits}
      face={<PartyFace conversation={conversation} size="list" channel={channel} />}
      name={<Name name={person.name} matched={conversation.matched} />}
      time={conversation.lastAt !== null && listTime(conversation.lastAt, timeZone, now)}
      wait={
        waits &&
        conversation.waitingSince !== null && <Wait since={conversation.waitingSince} now={now} />
      }
      line={<LastWritten conversation={conversation} typing={typing} />}
      below={below}
      open={() => select(conversation.id)}
      warm={() => warm(conversation.id)}
      done={() => done(conversation)}
    />
  );
}
/** What was written last in a conversation, under the first name of whoever at the DSP wrote it,
 * or who is writing in it now. */
function LastWritten({
  conversation,
  typing,
}: {
  conversation: Conversation;
  typing: readonly Writing[];
}) {
  const writing = typing.filter((writer) => writer.conversationId === conversation.id);
  if (writing.length) return <span className="preview typing-note">{typingNote(writing)}</span>;
  const { last } = conversation;
  const writer = last?.sender?.name.split(' ')[0];
  return (
    <span className="preview">
      {writer && <span className="preview-writer">{writer}: </span>}
      {last ? preview(last) : ''}
    </span>
  );
}
