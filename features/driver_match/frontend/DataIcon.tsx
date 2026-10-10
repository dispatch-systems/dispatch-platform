import type { DriverData } from '../api/index.js';
import { peopleIcon } from '../../../core/shell/frontend/runtime/slots.js';

/** A kind of collected data's icon, as the feature whose data it is declares it. */
export function DataIcon({ kind, size }: { kind: DriverData; size: number }) {
  const Icon = peopleIcon(kind);
  return Icon ? <Icon size={size} aria-hidden="true" /> : null;
}
