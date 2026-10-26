import { Phone, PhoneOff, Smartphone } from 'lucide-react';
import type { Conversation, Sender } from '../../api/index.js';
import { PersonAvatar } from '../../../../core/shell/frontend/runtime/PersonAvatar.js';
import { tone } from '../format.js';

/** Where someone is shown, which says how large. */
type Size = 'list' | 'contact' | 'sender' | 'head' | 'message' | 'profile';

/**
 * Someone in a conversation, as large as where they're shown (`size`): their photo as Driver
 * Match has it, or their initials in their tone until it comes or without one.
 */
export function Face({
  name,
  code,
  photo,
  size,
}: {
  name: string;
  code?: string | null;
  photo?: string | null;
  size: Size;
}) {
  return (
    <PersonAvatar
      name={name}
      code={code}
      photo={photo}
      className={`face face-${size} tone-${tone(name)}`}
    />
  );
}

/** What a phone in a person's place marks: a number no one is known by, the DSP's own phone, which
 * sent a text from its own messaging app, or a number blocked. */
const marks = { number: Phone, own: Smartphone, blocked: PhoneOff };

/** A phone in a person's place, as large as where it's shown, marking what it is (`mark`). */
export function PhoneFace({ size, mark = 'number' }: { size: Size; mark?: keyof typeof marks }) {
  const Icon = marks[mark];
  return (
    <span className={`face face-${size} face-phone`} aria-hidden="true">
      <Icon />
    </span>
  );
}

/** Whom a conversation is with, as large as where they're shown: by their number alone, a phone. */
export function Party({ conversation, size }: { conversation: Conversation; size: Size }) {
  const { with: person } = conversation;
  return conversation.kind === 'number' && !person.code && !conversation.contact ? (
    <PhoneFace size={size} />
  ) : (
    <Face {...person} size={size} />
  );
}

/** Who at the DSP sent a message, as large as where they're shown: by their photo, or the phone's
 * own mark for a text sent from its messaging app. */
export function SenderFace({ sender, size }: { sender: Sender; size: Size }) {
  return sender.userId ? (
    <Face name={sender.name} code={sender.code} photo={sender.photo} size={size} />
  ) : (
    <PhoneFace size={size} mark="own" />
  );
}
