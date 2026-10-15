import type { Driver } from '../../api/index.js';
import { initials, photoUrl, toneOf } from '../driver-match.js';

/** A person's photo from Amazon's roster, or their initials in their tone. */
export function DriverAvatar({
  driver,
  large = false,
}: {
  driver: Pick<Driver, 'code' | 'name' | 'photo'>;
  large?: boolean;
}) {
  const className = `driver-avatar tone-${toneOf(driver.code)}${large ? ' large' : ''}`;
  const photo = photoUrl(driver);
  return photo ? (
    <img className={className} src={photo} alt="" loading="lazy" />
  ) : (
    <span className={className} aria-hidden="true">
      {initials(driver.name)}
    </span>
  );
}
