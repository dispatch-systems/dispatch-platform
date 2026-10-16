import { usePicture } from '../../../../core/shell/frontend/runtime/usePicture.js';
import type { Driver } from '../../api/index.js';
import { initials, photoUrl, toneOf } from '../driver-match.js';

/** A person's photo from Amazon's roster, their initials in their tone until it comes. */
export function DriverAvatar({
  driver,
  size,
}: {
  driver: Pick<Driver, 'code' | 'name' | 'photo'>;
  /** Larger in a sheet's title, largest where a face finds them. */
  size?: 'large' | 'portrait';
}) {
  const [box, src, lost] = usePicture<HTMLSpanElement>(photoUrl(driver));
  return (
    <span
      ref={box}
      className={`driver-avatar tone-${toneOf(driver.code)}${size ? ` ${size}` : ''}`}
      aria-hidden="true"
    >
      {src ? <img src={src} alt="" onError={lost} /> : initials(driver.name)}
    </span>
  );
}
