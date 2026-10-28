import type { Conversation, ConversationKind, Elsewhere } from '../../api/index.js';
import { slackAttachments, textAttachments, type Attachments } from '../../api/client.js';
import type { Target } from './target.js';

// Each kind of conversation, defined once: how its messages carry photos and files, and how
// someone's conversation of the kind is known and started, by the account or number it's with.
// How whom it's with shows is parties.tsx's; the list's section for it is inbox/sections.ts's.

type Kind = {
  attachments: Attachments;
  /** Whether `conversation`, of this kind, is with whom `elsewhere` names. */
  isWith: (elsewhere: Elsewhere, conversation: Conversation) => boolean;
  /** What starts their conversation of this kind; none when `elsewhere` names too little. */
  starts: (elsewhere: Elsewhere) => Target | undefined;
};

export const KINDS: { [K in ConversationKind]: Kind } = {
  slack: {
    attachments: slackAttachments,
    isWith: (elsewhere, conversation) =>
      conversation.teamId === elsewhere.teamId && conversation.with.userId === elsewhere.userId,
    starts: ({ teamId, userId }) =>
      teamId && userId ? { kind: 'slack', teamId, userId } : undefined,
  },
  number: {
    attachments: textAttachments,
    isWith: (elsewhere, conversation) => conversation.number === elsewhere.number,
    starts: ({ number }) => (number ? { kind: 'number', number } : undefined),
  },
  work: {
    attachments: textAttachments,
    isWith: (elsewhere, conversation) => conversation.driverId === elsewhere.driverId,
    starts: ({ driverId }) => (driverId ? { kind: 'driver', driverId } : undefined),
  },
};
