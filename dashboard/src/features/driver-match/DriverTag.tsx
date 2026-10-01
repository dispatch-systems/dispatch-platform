import type { DriverStatus } from '../../../../shared/contracts/index.js';
import { statusLabels, statusTones } from '../../lib/driver-match.js';

export function DriverTag({ status }: { status: DriverStatus }) {
  return <span className={`driver-tag ${statusTones[status]}`}>{statusLabels[status]}</span>;
}
