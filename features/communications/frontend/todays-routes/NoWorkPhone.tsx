import { PhoneOff } from 'lucide-react';
import type { Elsewhere } from '../../api/index.js';
import { ElsewhereChip } from '../conversations/Elsewhere.js';
import './no-work-phone.css';

// In place of the box to write in, for a driver holding no work phone now, as one returned or never
// claimed, or once the day stopped: No work phone, and their other conversations to write in.

export function NoWorkPhone({
  elsewhere,
  go,
}: {
  elsewhere: readonly Elsewhere[];
  go: (elsewhere: Elsewhere) => void;
}) {
  return (
    <div className="no-work-phone">
      <span>
        <PhoneOff aria-hidden="true" />
        No work phone
      </span>
      {elsewhere
        .filter((each) => each.kind !== 'work')
        .map((each) => (
          <ElsewhereChip key={each.kind} elsewhere={each} go={go} />
        ))}
    </div>
  );
}
