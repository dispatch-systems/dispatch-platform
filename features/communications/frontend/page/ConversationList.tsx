import { useCallback } from 'react';
import { Check, Users } from 'lucide-react';
import type { Communications, Conversation, Group, Writing } from '../../api/index.js';
import { searchConversations } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { listTime, preview } from '../format.js';
import { rows as listed, type Row } from '../inbox.js';
import { Face } from './Face.js';
import type { Draft } from './GroupDraft.js';
import { GroupFaces } from './GroupFaces.js';
import { More } from './More.js';
import { Name, Wait, typingNote } from './parts.js';
import { useFound } from './useFound.js';

// Messages: the conversations and groups, of every workspace or the one chosen, those waiting on
// an answer, then those answered; or the conversations found by a name typed, misspelt or not. A
// conversation listed under a group is the group's row. The message being written to a group is
// at the top while it's written, and what's open at the top while the list doesn't show it, as
// one New message opened before it holds a message.

type Listing = {
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
  team,
  typed,
  waiting: waitingPeople,
  draft,
  pinned,
  readOlder,
  ...listing
}: Listing & {
  inbox: Communications;
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
    () => searchConversations(typed, team).then((found) => found.conversations),
    [typed, team],
  );
  const { found, failed } = useFound(typed ? search : undefined, team ?? '', inbox.revision, 200);
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
  const shown = listed(inbox, team);
  const waiting = shown.filter((each) => each.row.state === 'waiting');
  const answered = shown.filter((each) => each.row.state !== 'waiting');
  const pinnedShown =
    pinned &&
    (!team ||
      (pinned.kind === 'group' ? pinned.row.teamIds.includes(team) : pinned.row.teamId === team));
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
                <Face {...pinned.row.with} size="list" />
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
      {inbox.more && <More read={readOlder} />}
    </>
  );
}

/**
 * A conversation in the list: who it's with, when they last wrote, and what was written last, or
 * who at the DSP is writing in it. One waiting shows how long it has, and a check that marks it
 * done without a reply while it's pointed at.
 */
function ConversationRow({
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

/**
 * A group in the list: two of its people, its name and how many it went to, when it was last
 * written in, and what was, with its writer's first name. While anything under it waits, how
 * long the longest has, and a check that marks all of it done while it's pointed at.
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
  const selected = open === group.id;
  const last = group.last;
  const writer = last?.sender?.name ?? last?.from?.name ?? '';
  return (
    <li className={`convo group${selected ? ' selected' : ''}${waits ? ' waits' : ''}`}>
      <button
        className="convo-open"
        aria-current={selected || undefined}
        onClick={() => openGroup(group.id)}
      >
        <GroupFaces faces={group.faces} size="list" />
        <span className="convo-body">
          <span className="line1">
            <strong>
              {group.name}
              <span className="people"> · {group.people}</span>
            </strong>
            <time>{listTime(group.lastAt, timeZone, now)}</time>
          </span>
          <span className="line2">
            <span className="preview">
              {last ? `${writer.split(' ')[0] ?? writer}: ${preview(last)}` : ''}
            </span>
            {waits && group.waitingSince !== null && <Wait since={group.waitingSince} now={now} />}
          </span>
        </span>
      </button>
      {waits && (
        <button className="convo-done" aria-label="Done" onClick={() => doneGroup(group)}>
          <Check size={16} />
        </button>
      )}
    </li>
  );
}
