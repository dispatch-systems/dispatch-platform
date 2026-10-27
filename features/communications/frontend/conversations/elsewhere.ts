import type { Conversation, Elsewhere } from '../../api/index.js';

// Whom a conversation is with's conversations of each kind (`Elsewhere`), as the page goes
// between them: kept up to date by the list, and made from a conversation to go to.

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
/** A conversation as one to go to. */
export const asElsewhere = (conversation: Conversation): Elsewhere => ({
  kind: conversation.kind,
  conversationId: conversation.id,
  waiting: conversation.state === 'waiting',
  number: conversation.number,
  driverId: conversation.driverId,
  teamId: conversation.teamId,
  userId: conversation.with.userId,
});
