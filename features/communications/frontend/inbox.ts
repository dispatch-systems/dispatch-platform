import type {
  Communications,
  CommunicationsUpdates,
  Conversation,
  Conversations,
} from '../api/index.js';

// The page's conversations as they change: what the overview read, each update the page waits
// on, older answered ones as they're read, and what the page's own writes answer with. Those
// waiting come first, the longest waiting first, then those answered, the latest first: the
// order the server reads them in, so reading older ones goes on where the list stops.

/** Whether `a` is listed before `b`. */
function above(a: Conversation, b: Conversation) {
  const waits = a.state === 'waiting';
  if (waits !== (b.state === 'waiting')) return waits;
  if (waits)
    return a.waitingSince !== b.waitingSince
      ? (a.waitingSince ?? 0) < (b.waitingSince ?? 0)
      : a.id < b.id;
  return a.lastAt !== b.lastAt ? (a.lastAt ?? 0) > (b.lastAt ?? 0) : a.id > b.id;
}
const order = (a: Conversation, b: Conversation) => (a.id === b.id ? 0 : above(a, b) ? -1 : 1);

/** The last answered conversation listed, which reading older ones goes on from. */
export const lastAnswered = (inbox: Communications) =>
  inbox.conversations.findLast((conversation) => conversation.state !== 'waiting');

/**
 * The list with `changed` as they are now, in order. One with no message yet isn't listed; an
 * answered one below the last listed, while older ones are left to read, is read with them, so
 * none is missed or listed twice.
 */
function merged(inbox: Communications, changed: readonly Conversation[]) {
  const last = lastAnswered(inbox);
  const byId = new Map(inbox.conversations.map((conversation) => [conversation.id, conversation]));
  for (const conversation of changed) byId.set(conversation.id, conversation);
  return [...byId.values()]
    .filter((conversation) => conversation.lastAt !== null)
    .filter(
      (conversation) =>
        conversation.state === 'waiting' || !inbox.more || !last || !above(last, conversation),
    )
    .sort(order);
}

/** The list after an update the page waited on; one that resets is read afresh instead. */
export const updated = (inbox: Communications, update: CommunicationsUpdates): Communications => ({
  ...inbox,
  revision: update.revision,
  waiting: update.waiting,
  typing: update.typing,
  workspaces: update.workspaces,
  conversations: merged(inbox, update.conversations),
});
/** The list with the older answered conversations read after its last. */
export const withOlder = (inbox: Communications, older: Conversations): Communications => ({
  ...inbox,
  more: older.more,
  conversations: merged({ ...inbox, more: false }, older.conversations),
});
/** The list with a conversation as one of the page's own writes left it. */
export const withConversation = (
  inbox: Communications,
  conversation: Conversation,
): Communications => ({ ...inbox, conversations: merged(inbox, [conversation]) });

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
