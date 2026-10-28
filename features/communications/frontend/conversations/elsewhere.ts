import type { Conversation, Elsewhere } from '../../api/index.js';
import { KINDS } from './kinds.js';
import type { Target } from './target.js';

// Whom a conversation is with's conversations of each kind (`Elsewhere`), as the page goes
// between them: kept up to date by the list, and opened, or started.

/** Whether `conversation` is the one `elsewhere` stands for: by its ID, or for one not started as
 * it was read, by the driver, the number or the Slack account it's with. */
function standsFor(elsewhere: Elsewhere, conversation: Conversation) {
  if (elsewhere.conversationId !== null) return conversation.id === elsewhere.conversationId;
  return (
    conversation.kind === elsewhere.kind && KINDS[elsewhere.kind].isWith(elsewhere, conversation)
  );
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
/** What opens `elsewhere`, or starts it; none when it names too little. */
export const targetOf = (elsewhere: Elsewhere): Target | undefined =>
  elsewhere.conversationId
    ? { kind: 'conversation', id: elsewhere.conversationId }
    : KINDS[elsewhere.kind].starts(elsewhere);
