import { DriverAvatar } from '../parts/DriverAvatar.js';
import type { Check } from './checks.js';

/** How many faces the banner shows of the people it asks about. */
const FACES = 4;

/** How many questions wait, with some of the faces they ask about. */
export function CheckBanner({ checks, onStart }: { checks: Check[]; onStart: () => void }) {
  if (!checks.length) return null;
  // A side with a photo, when either has one, shows the face.
  const faces = checks
    .slice(0, FACES)
    .map((check) => check.sides.find((side) => side.photo) ?? check.sides[0]);
  return (
    <div className="driver-checks">
      <span className="driver-checks-faces">
        {faces.map((side, index) => (
          <DriverAvatar key={`${index}:${side.code}`} driver={side} />
        ))}
      </span>
      <strong>{checks.length} to check</strong>
      <button className="primary" onClick={onStart}>
        Start
      </button>
    </div>
  );
}
