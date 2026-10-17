import { PersonAvatar } from '../../../../core/shell/frontend/runtime/PersonAvatar.js';
import type { Driver } from '../../api/index.js';
import { photoUrl, toneOf } from '../driver-match.js';

/** A person's photo from Amazon's roster, their initials in their tone until it comes. */
export function DriverAvatar({
  driver,
  size,
}: {
  driver: Pick<Driver, 'code' | 'name' | 'photo'>;
  /** Larger in a sheet's title, largest where a face finds them. */
  size?: 'large' | 'portrait';
}) {
  return (
    <PersonAvatar
      name={driver.name}
      photo={photoUrl(driver)}
      className={`driver-avatar tone-${toneOf(driver.code)}${size ? ` ${size}` : ''}`}
    />
  );
}
