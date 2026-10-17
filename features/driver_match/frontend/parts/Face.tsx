import { PersonAvatar } from '../../../../core/shell/frontend/runtime/PersonAvatar.js';
import type { Person } from '../../api/index.js';
import { photoUrl, toneOf } from '../driver-match.js';

/** A person's photo, from whichever feature holds one, their initials in their tone until it comes. */
export function Face({
  person,
  size,
}: {
  person: Pick<Person, 'code' | 'name' | 'photo'>;
  /** Larger in a sheet's title, largest where a face finds them. */
  size?: 'large' | 'portrait';
}) {
  return (
    <PersonAvatar
      name={person.name}
      photo={photoUrl(person)}
      className={`driver-avatar tone-${toneOf(person.code)}${size ? ` ${size}` : ''}`}
    />
  );
}
