import { Fragment, useCallback, type ReactNode } from 'react';
import type { Communications, Conversation } from '../../api/index.js';
import { searchConversations } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { PartyFace } from '../conversations/party.js';
import { dayLabel, dayOf } from '../shared/format.js';
import { ListEmpty, ListHead, Rows } from '../shared/List.js';
import { ListRow } from '../shared/ListRow.js';
import { More } from '../shared/More.js';
import { useFound } from '../shared/useFound.js';
import type { Draft } from '../slack/groups/GroupDraft.js';
import { GroupDraftRow, GroupRow } from '../slack/groups/GroupRow.js';
import { ConversationRow } from './ConversationRow.js';
import { latestFirst, readsNext, rows as listed, type Row } from './inbox.js';
import {
  groupsSection,
  lists,
  sectionNamed,
  sectionOf,
  SECTIONS,
  type Section,
} from './sections.js';
import type { Listing } from './listing.js';

// Messages: a section's conversations, and in Slack's the groups, of every workspace or the one
// chosen, those waiting on an answer, then those answered; in Recents every section's, each face
// marked with where it is, those answered under the day each was last written in, and the latest
// written in first under each heading; or the conversations found by a name typed, misspelt or
// not, or by a number, in Recents in every section, the latest written in first. A conversation
// listed under a group is the group's row.
// The message being written to a group is at the top while it's written, and what's open at the
// top while the list doesn't show it, as one New message or Contacts opened before it holds a
// message.

export function ConversationList({
  inbox,
  section,
  team,
  typed,
  waiting: waitingPeople,
  draft,
  pinned,
  read,
  below,
  readOlder,
  olderFailed,
  ...listing
}: Listing & {
  inbox: Communications;
  section: Section;
  /** Conversations read elsewhere the list may not hold yet, as Today's Routes' read does. */
  read?: readonly Conversation[];
  /** What else a conversation's row shows, under what was written last, as a driver's route. */
  below?: (conversation: Conversation) => ReactNode;
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
  /** Whether the latest read of older ones failed. */
  olderFailed: boolean;
}) {
  const model = sectionNamed(section);
  // What Search searches: the section's own, or in Recents every section the DSP has.
  const searched = (
    model.everything
      ? SECTIONS.filter((each) => each.offered(inbox)).map((each) => each.id)
      : [section]
  ).join(' ');
  const search = useCallback(
    () =>
      Promise.all(
        searched
          .split(' ')
          .map((each) => searchConversations(typed, sectionNamed(each as Section).listing(team))),
      ).then((found) =>
        found.length > 1
          ? found.flatMap((each) => each.conversations).sort(latestFirst)
          : found[0]!.conversations,
      ),
    [typed, searched, team],
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
          <GroupRow key={each.row.id} group={each.row} channel={model.everything} {...listing} />
        ) : (
          <ConversationRow
            key={each.row.id}
            conversation={each.row}
            channel={model.everything}
            below={below?.(each.row)}
            {...listing}
          />
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
  const shown = listed(inbox, section, team, read);
  const pinnedShown =
    pinned &&
    (pinned.kind === 'group'
      ? lists(section, groupsSection) && (!team || pinned.row.teamIds.includes(team))
      : lists(section, sectionOf(pinned.row)) && (!team || pinned.row.teamId === team));
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
              face={<PartyFace conversation={pinned.row} size="list" />}
              name={pinned.row.with.name}
              line={<span className="preview">New message</span>}
              open={() => listing.select(pinned.row.id)}
            />
          </Rows>
        ) : (
          list([pinned])
        ))}
      {byState(
        shown,
        waitingPeople,
        model.everything ? (answered) => byDay(answered, listing) : responded,
      ).map(({ key, label, count, rows }) => (
        <Fragment key={key}>
          <ListHead count={count}>{label}</ListHead>
          {list(rows)}
        </Fragment>
      ))}
      {readsNext(inbox, section) && <More read={readOlder} failed={olderFailed} />}
    </>
  );
}

/** A list's rows under their headings, with how many people wait under Waiting. */
type Headed = { key: string; label: string; count?: number; rows: Row[] };
/** Those waiting, then those answered, under the headings `answered` gives them. */
function byState(
  shown: readonly Row[],
  waiting: number,
  answered: (rows: Row[]) => Headed[],
): Headed[] {
  const headed = [
    {
      key: 'waiting',
      label: 'Waiting',
      count: waiting,
      rows: shown.filter((each) => each.row.state === 'waiting'),
    },
    ...answered(shown.filter((each) => each.row.state !== 'waiting')),
  ];
  return headed.filter((each) => each.rows.length > 0);
}
/** Those answered, under Responded. */
const responded = (rows: Row[]): Headed[] => [{ key: 'responded', label: 'Responded', rows }];
/** Under the day each was last written in: Today, Yesterday, then by the day. */
function byDay(shown: readonly Row[], { timeZone, now }: Pick<Listing, 'timeZone' | 'now'>) {
  const headed: Headed[] = [];
  for (const each of shown) {
    const at = each.row.lastAt ?? now;
    const key = dayOf(at, timeZone);
    const last = headed.at(-1);
    if (last?.key === key) last.rows.push(each);
    else headed.push({ key, label: dayLabel(at, timeZone, now), rows: [each] });
  }
  return headed;
}
