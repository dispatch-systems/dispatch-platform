import { initials, toneOf } from './driver-match.js';

export function DriverAvatar({
  name,
  code,
  large = false,
}: {
  name: string;
  code: string;
  large?: boolean;
}) {
  return (
    <span
      className={`driver-avatar tone-${toneOf(code)}${large ? ' large' : ''}`}
      aria-hidden="true"
    >
      {initials(name)}
    </span>
  );
}
