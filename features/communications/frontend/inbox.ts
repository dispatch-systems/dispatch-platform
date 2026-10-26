import type {
  Communications,
  CommunicationsUpdates,
  CommunicationsWorkspace,
  Conversation,
  Conversations,
  Group,
} from '../api/index.js';

// The page's conversations and groups as they change: what the overview read, each update the
// page waits on, older answered ones as they're read, and what the page's own writes answer with.
// Those waiting come first, the longest waiting first, then those answered, the latest first: the
// order the server reads them in, so reading older ones goes on where each section's list stops.
// A conversation listed under a group is the group's row, not one of its own. Each is listed in a
// section: a number's texts in Messaging, a Slack direct message and the groups in Slack; a
// blocked number's in none.

/** A section of the list. */
export type Section = 'messaging' | 'slack';
/** The section a conversation is listed in, if any. */
export function sectionOf(conversation: Conversation): Section | undefined {
  if (conversation.kind === 'slack') return 'slack';
  if (conversation.kind === 'number' && !conversation.blocked) return 'messaging';
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
/** How many conversations wait in `section`, those under a group too. */
export const waitingIn = (inbox: Communications, section: Section) =>
  inbox.conversations.filter(
    (conversation) => conversation.state === 'waiting' && sectionOf(conversation) === section,
  ).length;
/** Whether older answered rows of `section` are left to read. */
const moreIn = (inbox: Communications, section: Section) =>
  section === 'slack' ? inbox.more : inbox.moreTexts;
const sections: readonly Section[] = ['messaging', 'slack'];

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
        return row.state === 'waiting' || !among || !moreIn(inbox, among) || !after || !above(after, row);
      })
      .sort(order);
  };
  return {
    conversations: kept(inbox.conversations, conversations, sectionOf),
    groups: kept(inbox.groups, groups, () => 'slack'),
  };
}

/** The list after an update the page waited on; one that resets is read afresh instead. */
export const updated = (inbox: Communications, update: CommunicationsUpdates): Communications => ({
  ...inbox,
  revision: update.revision,
  waiting: update.waiting,
  typing: update.typing,
  workspaces: update.workspaces,
  texting: update.texting,
  ...merged(inbox, update.conversations, update.groups),
});
/** The list with the older answered conversations and groups of `section` read after its last. */
export function withOlder(
  inbox: Communications,
  older: Conversations,
  section: Section,
): Communications {
  const more = section === 'slack' ? 'more' : 'moreTexts';
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
