import { Phone, Smartphone } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
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

/** A phone in a person's place: a number no one is known by, or the DSP's own phone, which sent a
 * text from its own messaging app. */
export function PhoneFace({ size, own = false }: { size: Size; own?: boolean }) {
  const Icon = own ? Smartphone : Phone;
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
