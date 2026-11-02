import type { ReactNode } from 'react';
import { Phone, Route } from 'lucide-react';
import type { Conversation, ConversationKind } from '../../api/index.js';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { Face, PhoneFace, type Size } from '../shared/Face.js';
import './party.css';

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

/** Where a conversation is, as its face is marked in Recents: Today's Routes, by text, or Slack. */
const channels: Record<ConversationKind, ReactNode> = {
  work: <Route aria-hidden="true" />,
  number: <Phone aria-hidden="true" />,
  slack: <SlackLogo size={10} />,
};

/** Whom a conversation is with, as large as where they're shown: their face, marked with where
 * the conversation is while `channel`, or a phone for a number no one is known by and a work phone
 * nobody held, which say so themselves. */
export function PartyFace({
  conversation,
  size,
  channel = false,
}: {
  conversation: Conversation;
  size: Size;
  channel?: boolean;
}) {
  const { with: person } = conversation;
  switch (partyOf(conversation)) {
    case 'work_phone':
      return <PhoneFace size={size} mark="work" />;
    case 'number':
      if (!person.code && !conversation.contact) return <PhoneFace size={size} />;
  }
  const face = <Face {...person} size={size} />;
  if (!channel) return face;
  return (
    <span className="face-channel">
      {face}
      <span className="channel-mark">{channels[conversation.kind]}</span>
    </span>
  );
}
