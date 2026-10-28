import type { Correspondent } from '../../api/index.js';
import { existingOr, type Target } from '../conversations/target.js';

/** What opens a member's conversation in workspace `teamId`, or starts it; none for someone with
 * no account there. */
export const memberTarget = ({
  teamId,
  person,
  conversationId,
}: {
  teamId: string;
  person: Correspondent;
  conversationId: string | null;
}): Target | undefined =>
  person.userId
    ? existingOr(conversationId, { kind: 'slack', teamId, userId: person.userId })
    : undefined;
