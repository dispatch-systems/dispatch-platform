import { useCallback } from 'react';
import type { Communications } from '../../api/index.js';
import { searchConversations } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { Party } from '../shared/Face.js';
import { ListEmpty, ListHead, Rows } from '../shared/List.js';
import { ListRow } from '../shared/ListRow.js';
import { More } from '../shared/More.js';
import { useFound } from '../shared/useFound.js';
import type { Draft } from '../slack/groups/GroupDraft.js';
import { GroupDraftRow, GroupRow } from '../slack/groups/GroupRow.js';
import { ConversationRow } from './ConversationRow.js';
import { listingOf, moreIn, rows as listed, sectionOf, type Row, type Section } from './inbox.js';
import type { Listing } from './listing.js';

// Messages: a section's conversations, and in Slack's the groups, of every workspace or the one
// chosen, those waiting on an answer, then those answered; or the conversations found by a name
// typed, misspelt or not, or in Messaging by a number. A conversation listed under a group is the
// group's row. The message being written to a group is at the top while it's written, and what's
// open at the top while the list doesn't show it, as one New message or Contacts opened before it
// holds a message.

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
    <Rows>
      {rows.map((each) =>
        each.kind === 'group' ? (
          <GroupRow key={each.row.id} group={each.row} {...listing} />
        ) : (
          <ConversationRow key={each.row.id} conversation={each.row} {...listing} />
        ),
      )}
    </Rows>
  );
  if (typed) {
    if (failed || found?.length === 0) return <ListEmpty>{failed || 'No matches'}</ListEmpty>;
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
        <Rows>
          <GroupDraftRow draft={draft} />
        </Rows>
      )}
      {pinnedShown &&
        (pinned.kind === 'conversation' && pinned.row.lastAt === null ? (
          <Rows>
            <ListRow
              selected
              waits={false}
              face={<Party conversation={pinned.row} size="list" />}
              name={pinned.row.with.name}
              line={<span className="preview">New message</span>}
              open={() => listing.select(pinned.row.id)}
            />
          </Rows>
        ) : (
          list([pinned])
        ))}
      {waiting.length > 0 && (
        <>
          <ListHead count={waitingPeople}>Waiting</ListHead>
          {list(waiting)}
        </>
      )}
      {answered.length > 0 && (
        <>
          <ListHead>Responded</ListHead>
          {list(answered)}
        </>
      )}
      {moreIn(inbox, section) && <More read={readOlder} />}
    </>
  );
}
