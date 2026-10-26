import type {
  Communications,
  CommunicationsUpdates,
  CommunicationsWorkspace,
  Conversation,
  Conversations,
  Elsewhere,
  Group,
} from '../api/index.js';
import type { Listing } from '../api/client.js';

// The page's conversations and groups as they change: what the overview read, each update the
// page waits on, older answered ones as they're read, and what the page's own writes answer with.
// Those waiting come first, the longest waiting first, then those answered, the latest first: the
// order the server reads them in, so reading older ones goes on where each section's list stops.
// A conversation listed under a group is the group's row, not one of its own. Each is listed in a
// section: a work phone's texts in Today's Routes, which lists them by its own read
// (`todays-routes.ts`); a number's texts in Messaging; a Slack direct message and the groups in
// Slack; a blocked number's in none.

/** A section of the list, in the order the switch shows them. */
export const sections = ['routes', 'messaging', 'slack'] as const;
export type Section = (typeof sections)[number];
/** The section a conversation is listed in, if any. */
export function sectionOf(conversation: Conversation): Section | undefined {
  if (conversation.kind === 'slack') return 'slack';
  if (conversation.kind === 'work') return 'routes';
  if (!conversation.blocked) return 'messaging';
  return undefined;
}

/** A row of the list: a conversation, or a group. */
type Listed = Pick<Conversation, 'id' | 'state' | 'waitingSince'> & { lastAt: number | null };

/** Whether `a` is listed before `b`. */
function above(a: Listed, b: Listed) {
  const waits = a.state === 'waiting';
  if (waits !== (b.state === 'waiting')) return waits;
  if (waits)
    return a.waitingSince !== b.waitingSince
      ? (a.waitingSince ?? 0) < (b.waitingSince ?? 0)
      : a.id < b.id;
  return a.lastAt !== b.lastAt ? (a.lastAt ?? 0) > (b.lastAt ?? 0) : a.id > b.id;
}
export const order = (a: Listed, b: Listed) => (a.id === b.id ? 0 : above(a, b) ? -1 : 1);

/** Whether a conversation is a row of its own in `section`: not listed under a group. */
const rowIn = (section: Section) => (conversation: Conversation) =>
  conversation.groupId === null && sectionOf(conversation) === section;
/** How many conversations wait in each section, those under a group too. */
export function waitingIn(inbox: Communications): Record<Section, number> {
  const waiting = { routes: 0, messaging: 0, slack: 0 };
  for (const conversation of inbox.conversations) {
    const section = sectionOf(conversation);
    if (conversation.state === 'waiting' && section) waiting[section]++;
  }
  return waiting;
}
/** Where the list says whether older answered rows of a section that pages are left to read:
 * Today's Routes is read whole. */
const moreOf = (section: Exclude<Section, 'routes'>) =>
  section === 'slack' ? 'more' : 'moreTexts';
/** Whether older answered rows of `section` are left to read. */
export const moreIn = (inbox: Communications, section: Section) =>
  section !== 'routes' && inbox[moreOf(section)];
/** Where `section`'s conversations are read from: in Slack's, of every workspace or `team`'s. */
export const listingOf = (section: Section, team?: string): Listing =>
  section === 'slack' ? { team } : { kind: section === 'routes' ? 'work' : 'number' };

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
  [...inbox.conversations.filter(rowIn(section)), ...(section === 'slack' ? inbox.groups : [])]
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
  const last = new Map(sections.map((section) => [section, lastAnswered(inbox, section)]));
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
    groups: kept(inbox.groups, groups, () => 'slack'),
  };
}

/** The list after an update the page waited on, with all else it brings as it is now, as the
 * phones and whether Today's Routes is offered; one that resets is read afresh instead. */
export function updated(inbox: Communications, update: CommunicationsUpdates): Communications {
  const { reset: _reset, conversations, groups, ...now } = update;
  return { ...inbox, ...now, ...merged(inbox, conversations, groups) };
}
/** The list with the older answered conversations and groups of `section` read after its last. */
export function withOlder(
  inbox: Communications,
  older: Conversations,
  section: Exclude<Section, 'routes'>,
): Communications {
  const more = moreOf(section);
  return {
    ...inbox,
    [more]: older.more,
    ...merged({ ...inbox, [more]: false }, older.conversations, older.groups),
  };
}
/** The list with a conversation as one of the page's own writes left it. */
export const withConversation = (
  inbox: Communications,
  conversation: Conversation,
): Communications => ({ ...inbox, ...merged(inbox, [conversation], []) });
/** The list with a group as one of the page's own writes left it. */
export const withGroup = (inbox: Communications, group: Group): Communications => ({
  ...inbox,
  ...merged(inbox, [], [group]),
});

/** Whether `conversation` is the one `elsewhere` stands for: by its ID, or for one not started as
 * it was read, by the driver, the number or the Slack account it's with. */
function standsFor(elsewhere: Elsewhere, conversation: Conversation) {
  if (elsewhere.conversationId !== null) return conversation.id === elsewhere.conversationId;
  if (conversation.kind !== elsewhere.kind) return false;
  if (elsewhere.kind === 'work') return conversation.driverId === elsewhere.driverId;
  if (elsewhere.kind === 'number') return conversation.number === elsewhere.number;
  return conversation.teamId === elsewhere.teamId && conversation.with.userId === elsewhere.userId;
}
/** Whom a conversation is with's conversations of each kind, as the list has them now, which each
 * update keeps: one waits while the list holds it waiting, as it holds every one that does, and one
 * started since they were read is theirs. */
export const elsewhereNow = (
  elsewhere: readonly Elsewhere[],
  conversations: readonly Conversation[],
): Elsewhere[] =>
  elsewhere.map((each) => {
    const listed = conversations.find((conversation) => standsFor(each, conversation));
    return {
      ...each,
      conversationId: listed?.id ?? each.conversationId,
      waiting: listed?.state === 'waiting',
    };
  });

/** The rows of `section`'s list, of every workspace or `team`'s: each conversation of its own, and
 * in Slack's each group with someone in that workspace, in order. */
export function rows(inbox: Communications, section: Section, team?: string) {
  const conversations = inbox.conversations
    .filter(rowIn(section))
    .filter((conversation) => !team || conversation.teamId === team)
    .map((conversation) => ({ kind: 'conversation' as const, row: conversation }));
  const groups = (section === 'slack' ? inbox.groups : [])
    .filter((group) => !team || group.teamIds.includes(team))
    .map((group) => ({ kind: 'group' as const, row: group }));
  return [...conversations, ...groups].sort((a, b) => order(a.row, b.row));
}
export type Row = ReturnType<typeof rows>[number];

/** `name` in runs, each marked when its letters matched what was searched (`matched`). */
export function marked(name: string, matched: readonly (readonly [number, number])[]) {
  const runs: { text: string; mark: boolean }[] = [];
  let at = 0;
  for (const [start, end] of matched) {
    if (start > at) runs.push({ text: name.slice(at, start), mark: false });
    runs.push({ text: name.slice(start, end), mark: true });
    at = end;
  }
  if (at < name.length) runs.push({ text: name.slice(at), mark: false });
  return runs;
}
