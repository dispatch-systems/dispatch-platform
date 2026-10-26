import { useCallback, type ReactNode } from 'react';
import { Users } from 'lucide-react';
import type { Communications, Conversation, Group, Writing } from '../../api/index.js';
import { searchConversations } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { listTime, preview } from '../format.js';
import { listingOf, moreIn, rows as listed, sectionOf, type Row, type Section } from '../inbox.js';
import { Party, SenderFace } from './Face.js';
import type { Draft } from './GroupDraft.js';
import { GroupFaces } from './GroupFaces.js';
import { ListRow } from './ListRow.js';
import { More } from './More.js';
import { Name, Wait, typingNote } from './parts.js';
import { useFound } from './useFound.js';

// Messages: a section's conversations, and in Slack's the groups, of every workspace or the one
// chosen, those waiting on an answer, then those answered; or the conversations found by a name
// typed, misspelt or not, or in Messaging by a number. A conversation listed under a group is the
// group's row. The message being written to a group is at the top while it's written, and what's
// open at the top while the list doesn't show it, as one New message or Contacts opened before it
// holds a message.

export type Listing = {
  /** What's open: a conversation or a group, by its ID. */
  open?: string;
  typing: readonly Writing[];
  timeZone: string;
  now: number;
  select: (id: string) => void;
  openGroup: (id: string) => void;
  done: (conversation: Conversation) => void;
  doneGroup: (group: Group) => void;
};

export function ConversationList({
  inbox,
  section,
  team,
  typed,
  waiting: waitingPeople,
  draft,
  pinned,
  readOlder,
  ...listing
}: Listing & {
  inbox: Communications;
  section: Section;
  /** The workspace chosen, if one is. */
  team?: string;
  /** What's typed in Search, trimmed. */
  typed: string;
  /** How many people wait, in the workspace chosen or every one. */
  waiting: number;
  /** The message being written to a group, while it is. */
  draft?: Draft;
  /** What's open, while the list doesn't show it. */
  pinned?: Row;
  readOlder: () => void;
}) {
  const search = useCallback(
    () => searchConversations(typed, listingOf(section, team)).then((found) => found.conversations),
    [typed, section, team],
  );
  const { found, failed } = useFound(
    typed ? search : undefined,
    `${section} ${team ?? ''}`,
    inbox.revision,
    200,
  );
  const list = (rows: readonly Row[]) => (
    <ul className="convos">
      {rows.map((each) =>
        each.kind === 'group' ? (
          <GroupRow key={each.row.id} group={each.row} {...listing} />
        ) : (
          <ConversationRow key={each.row.id} conversation={each.row} {...listing} />
        ),
      )}
    </ul>
  );
  if (typed) {
    if (failed || found?.length === 0)
      return <p className="list-empty">{failed || 'No matches'}</p>;
    return found ? (
      list(found.map((conversation) => ({ kind: 'conversation', row: conversation })))
    ) : (
      <Loading />
    );
  }
  const shown = listed(inbox, section, team);
  const waiting = shown.filter((each) => each.row.state === 'waiting');
  const answered = shown.filter((each) => each.row.state !== 'waiting');
  const pinnedShown =
    pinned &&
    (pinned.kind === 'group'
      ? section === 'slack' && (!team || pinned.row.teamIds.includes(team))
      : sectionOf(pinned.row) === section && (!team || pinned.row.teamId === team));
  return (
    <>
      {draft && (
        <ul className="convos">
          <li className="convo selected draft">
            <span className="convo-open">
              <span className="face face-list draft-face">
                <Users aria-hidden="true" />
              </span>
              <span className="convo-body">
                <span className="line1">
                  <strong>
                    {draft.name || 'New message'}
                    {draft.name && <span className="people"> · {draft.people}</span>}
                  </strong>
                </span>
              </span>
            </span>
          </li>
        </ul>
      )}
      {pinnedShown &&
        (pinned.kind === 'conversation' && pinned.row.lastAt === null ? (
          <ul className="convos">
            <li className="convo selected">
              <button className="convo-open" onClick={() => listing.select(pinned.row.id)}>
                <Party conversation={pinned.row} size="list" />
                <span className="convo-body">
                  <span className="line1">
                    <strong>{pinned.row.with.name}</strong>
                  </span>
                  <span className="line2">
                    <span className="preview">New message</span>
                  </span>
                </span>
              </button>
            </li>
          </ul>
        ) : (
          list([pinned])
        ))}
      {waiting.length > 0 && (
        <>
          <h2 className="list-head">
            Waiting<span>{waitingPeople}</span>
          </h2>
          {list(waiting)}
        </>
      )}
      {answered.length > 0 && (
        <>
          <h2 className="list-head">Responded</h2>
          {list(answered)}
        </>
      )}
      {moreIn(inbox, section) && <More read={readOlder} />}
    </>
  );
}

/**
 * A conversation in the list: who it's with, when they last wrote, and what was written last, or
 * who at the DSP is writing in it; while it waits, how long it has, and Done.
 */
export function ConversationRow({
  conversation,
  open,
  typing,
  timeZone,
  now,
  select,
  done,
  below,
}: Pick<Listing, 'open' | 'typing' | 'timeZone' | 'now' | 'select' | 'done'> & {
  conversation: Conversation;
  /** What else the row shows, under what was written last. */
  below?: ReactNode;
}) {
  const { with: person } = conversation;
  const waits = conversation.state === 'waiting';
  return (
    <ListRow
      selected={open === conversation.id}
      waits={waits}
      face={<Party conversation={conversation} size="list" />}
      name={<Name name={person.name} matched={conversation.matched} />}
      time={conversation.lastAt !== null && listTime(conversation.lastAt, timeZone, now)}
      wait={
        waits &&
        conversation.waitingSince !== null && <Wait since={conversation.waitingSince} now={now} />
      }
      line={<Written conversation={conversation} typing={typing} />}
      below={below}
      open={() => select(conversation.id)}
      done={() => done(conversation)}
    />
  );
}
/** What was written last in a conversation, under who at the DSP wrote it, or who is writing in it
 * now. */
function Written({
  conversation,
  typing,
}: {
  conversation: Conversation;
  typing: readonly Writing[];
}) {
  const writing = typing.filter((writer) => writer.conversationId === conversation.id);
  if (writing.length) return <span className="preview typing-note">{typingNote(writing)}</span>;
  const { last } = conversation;
  return (
    <>
      {last?.sender && <SenderFace sender={last.sender} size="sender" />}
      <span className="preview">{last ? preview(last) : ''}</span>
    </>
  );
}

/**
 * A group in the list: two of its people, its name and how many it went to, when it was last
 * written in, and what was, with its writer's first name. While anything under it waits, how
 * long the longest has, and a check that marks all of it done.
 */
function GroupRow({
  group,
  open,
  timeZone,
  now,
  openGroup,
  doneGroup,
}: Listing & { group: Group }) {
  const waits = group.state === 'waiting';
  const last = group.last;
  const writer = last?.sender?.name ?? last?.from?.name ?? '';
  return (
    <ListRow
      className="group"
      selected={open === group.id}
      waits={waits}
      face={<GroupFaces faces={group.faces} size="list" />}
      name={
        <>
          {group.name}
          <span className="people"> · {group.people}</span>
        </>
      }
      time={listTime(group.lastAt, timeZone, now)}
      wait={waits && group.waitingSince !== null && <Wait since={group.waitingSince} now={now} />}
      line={
        <span className="preview">
          {last ? `${writer.split(' ')[0] ?? writer}: ${preview(last)}` : ''}
        </span>
      }
      open={() => openGroup(group.id)}
      done={() => doneGroup(group)}
    />
  );
}
