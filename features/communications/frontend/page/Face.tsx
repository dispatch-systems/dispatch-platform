import { PersonAvatar } from '../../../../core/shell/frontend/runtime/PersonAvatar.js';
import { tone } from '../format.js';

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
  size: 'list' | 'contact' | 'sender' | 'head' | 'message' | 'profile';
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
