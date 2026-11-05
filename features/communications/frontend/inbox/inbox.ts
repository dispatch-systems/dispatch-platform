import type {
  Communications,
  CommunicationsUpdates,
  CommunicationsWorkspace,
  Conversation,
  Conversations,
  Group,
} from '../../api/index.js';
import {
  groupsSection,
  lists,
  sectionNamed,
  sectionOf,
  SECTIONS,
  type Section,
} from './sections.js';

// The page's conversations and groups as they change: what the overview read, each update the
// page waits on, older answered ones as they're read, and what the page's own writes answer with.
// Those waiting come first, the longest waiting first, then those answered, the latest first: the
// order the server reads them in, so reading older ones goes on where each section's list stops.
// A conversation listed under a group is the group's row, not one of its own. Each is listed in
// the section it's its own of (`sections.ts`), and the groups in theirs; Recents lists every
// section's, the latest written in first, down to where the first of them to stop stops; Today's
// Routes lists its own by its own read (`todays-routes/todays-routes.ts`).

/** A row of the list: a conversation, or a group. */
type Listed = Pick<Conversation, 'id' | 'state' | 'waitingSince'> & { lastAt: number | null };

/** Whether `a` was written in after `b`. */
const later = (a: Listed, b: Listed) =>
  a.lastAt !== b.lastAt ? (a.lastAt ?? 0) > (b.lastAt ?? 0) : a.id > b.id;
/** Whether `a` is listed before `b`. */
function above(a: Listed, b: Listed) {
  const waits = a.state === 'waiting';
  if (waits !== (b.state === 'waiting')) return waits;
  if (waits)
    return a.waitingSince !== b.waitingSince
      ? (a.waitingSince ?? 0) < (b.waitingSince ?? 0)
      : a.id < b.id;
  return later(a, b);
}
export const order = (a: Listed, b: Listed) => (a.id === b.id ? 0 : above(a, b) ? -1 : 1);
/** The latest written in first, waiting or not, as Recents lists them. */
export const latestFirst = (a: Listed, b: Listed) => (a.id === b.id ? 0 : later(a, b) ? -1 : 1);

/** Whether a conversation is a row of `section`'s list: listed there, not under a group. */
const rowIn = (section: Section) => (conversation: Conversation) =>
  conversation.groupId === null && lists(section, sectionOf(conversation));
/** Whether a conversation is one of `section`'s own rows, as its pages are read. */
const ownRowIn = (section: Section) => (conversation: Conversation) =>
  conversation.groupId === null && sectionOf(conversation) === section;
/** How many conversations wait in each section's list, those under a group too. */
export function waitingIn(inbox: Communications): Record<Section, number> {
  const waiting = Object.fromEntries(SECTIONS.map((each) => [each.id, 0])) as Record<
    Section,
    number
  >;
  for (const conversation of inbox.conversations) {
    if (conversation.state !== 'waiting') continue;
    const of = sectionOf(conversation);
    for (const { id } of SECTIONS) if (lists(id, of)) waiting[id]++;
  }
  return waiting;
}
/** Whether older answered rows of `section`'s own are left to read: never of one read whole. */
export const moreIn = (inbox: Communications, section: Section) => {
  const { more } = sectionNamed(section);
  return more ? inbox[more] : false;
};
/**
 * The section whose older answered rows `section`'s list reads next, while any are left: its own;
 * for one listing every section's, of those with older ones left, the one whose last listed was
 * written in latest, as the list stops there until they're read.
 */
export function readsNext(inbox: Communications, section: Section): Section | undefined {
  if (!sectionNamed(section).everything) return moreIn(inbox, section) ? section : undefined;
  let next: { section: Section; last: Listed } | undefined;
  for (const { id } of SECTIONS) {
    const last = moreIn(inbox, id) ? lastAnswered(inbox, id) : undefined;
    if (last && (!next || later(last, next.last))) next = { section: id, last };
  }
  return next?.section;
}

/** The workspace `chosen`, while it's one of two or more Communications works in; else none,
 * for every one. */
export function shownWorkspace(
  chosen: string | undefined,
  workspaces: readonly CommunicationsWorkspace[] = [],
) {
  const working = workspaces.filter((workspace) => workspace.on);
  return working.length > 1 && working.some((workspace) => workspace.teamId === chosen)
    ? chosen
    : undefined;
}

/** The last answered row listed in `section`, a conversation or a group, which reading older
 * ones goes on from. */
export const lastAnswered = (inbox: Communications, section: Section): Listed | undefined =>
  [
    ...inbox.conversations.filter(ownRowIn(section)),
    ...(sectionNamed(section).groups ? inbox.groups : []),
  ]
    .filter((row) => row.state !== 'waiting')
    .sort(order)
    .at(-1);

/**
 * The list with `conversations` and `groups` as they are now, in order. A conversation with no
 * message yet isn't listed; an answered row below the last listed in its section, while older
 * ones are left to read there, is read with them, so none is missed or listed twice.
 */
function merged(
  inbox: Communications,
  conversations: readonly Conversation[],
  groups: readonly Group[],
): Pick<Communications, 'conversations' | 'groups'> {
  const last = new Map(SECTIONS.map(({ id }) => [id, lastAnswered(inbox, id)]));
  const kept = <T extends Listed>(
    rows: readonly T[],
    changed: readonly T[],
    section: (row: T) => Section | undefined,
  ) => {
    const byId = new Map(rows.map((row) => [row.id, row]));
    for (const row of changed) byId.set(row.id, row);
    return [...byId.values()]
      .filter((row) => row.lastAt !== null)
      .filter((row) => {
        const among = section(row);
        const after = among && last.get(among);
        return (
          row.state === 'waiting' || !among || !moreIn(inbox, among) || !after || !above(after, row)
        );
      })
      .sort(order);
  };
  return {
    conversations: kept(inbox.conversations, conversations, sectionOf),
    groups: kept(inbox.groups, groups, () => groupsSection),
  };
}

/** The list after an update the page waited on, with all else it brings as it is now, as the
 * phones and whether Today's Routes is offered; one that resets is read afresh instead. */
export function updated(inbox: Communications, update: CommunicationsUpdates): Communications {
  const { reset: _reset, conversations, groups, ...now } = update;
  return { ...inbox, ...now, ...merged(inbox, conversations, groups) };
}
/** The list with the older answered conversations and groups of `section` read after its last:
 * one that pages. */
export function withOlder(
  inbox: Communications,
  older: Conversations,
  section: Section,
): Communications {
  const { more } = sectionNamed(section);
  if (!more) return inbox;
  return {
    ...inbox,
    [more]: older.more,
    ...merged({ ...inbox, [more]: false }, older.conversations, older.groups),
  };
}
/** `written`, as one of the page's own writes answered it, unless `kept` was written in since, as
 * by a message an update brought while the write went. */
export const newer = (kept: Conversation | undefined, written: Conversation) =>
  kept && (kept.lastAt ?? 0) > (written.lastAt ?? 0) ? kept : written;
/** The list with a conversation as one of the page's own writes left it. */
export const withConversation = (
  inbox: Communications,
  conversation: Conversation,
): Communications => {
  const kept = inbox.conversations.find((each) => each.id === conversation.id);
  return { ...inbox, ...merged(inbox, [newer(kept, conversation)], []) };
};
/** The list with a group as one of the page's own writes left it. */
export const withGroup = (inbox: Communications, group: Group): Communications => ({
  ...inbox,
  ...merged(inbox, [], [group]),
});

/**
 * The rows of `section`'s list, of every workspace or `team`'s: each conversation of its own, with
 * those written in that `read` holds and the list doesn't, as Today's Routes' read does, and the
 * groups where it lists them, each with someone in that workspace; in order, or in Recents the
 * latest written in first, down to the last listed of the section it reads older ones of next.
 */
export function rows(
  inbox: Communications,
  section: Section,
  team?: string,
  read: readonly Conversation[] = [],
) {
  const held = new Set(inbox.conversations.map((conversation) => conversation.id));
  const conversations = [
    ...inbox.conversations,
    ...read.filter((conversation) => !held.has(conversation.id) && conversation.lastAt !== null),
  ]
    .filter(rowIn(section))
    .filter((conversation) => !team || conversation.teamId === team)
    .map((conversation) => ({ kind: 'conversation' as const, row: conversation }));
  const groups = (lists(section, groupsSection) ? inbox.groups : [])
    .filter((group) => !team || group.teamIds.includes(team))
    .map((group) => ({ kind: 'group' as const, row: group }));
  const listed = [...conversations, ...groups];
  if (!sectionNamed(section).everything) return listed.sort((a, b) => order(a.row, b.row));
  const next = readsNext(inbox, section);
  const until = next && lastAnswered(inbox, next);
  return listed
    .filter(({ row }) => row.state === 'waiting' || !until || !above(until, row))
    .sort((a, b) => latestFirst(a.row, b.row));
}
export type Row = ReturnType<typeof rows>[number];
