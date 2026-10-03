import type { DriverStatus } from '../../../shared/contracts/driver-match.js';
import { statusLabels, statusTones } from './driver-match.js';

export function DriverTag({ status }: { status: DriverStatus }) {
  return <span className={`driver-tag ${statusTones[status]}`}>{statusLabels[status]}</span>;
}
