import type { Conversation } from '../../api/index.js';
import { Face, PhoneFace, type Size } from '../shared/Face.js';

/**
 * Whom a conversation is with, as the page shows them: a member of a workspace, a number, a
 * driver by their work conversation, or a work phone nobody held. What the pane does for each is
 * parties.tsx's.
 */
export type Party = 'member' | 'number' | 'driver' | 'work_phone';
export function partyOf(conversation: Conversation): Party {
  switch (conversation.kind) {
    case 'slack':
      return 'member';
    case 'number':
      return 'number';
    case 'work':
      return conversation.driverId ? 'driver' : 'work_phone';
  }
}

/** Whom a conversation is with, as large as where they're shown: their face, or a phone for a
 * number no one is known by and a work phone nobody held. */
export function PartyFace({ conversation, size }: { conversation: Conversation; size: Size }) {
  const { with: person } = conversation;
  switch (partyOf(conversation)) {
    case 'work_phone':
      return <PhoneFace size={size} mark="work" />;
    case 'number':
      if (!person.code && !conversation.contact) return <PhoneFace size={size} />;
  }
  return <Face {...person} size={size} />;
}
