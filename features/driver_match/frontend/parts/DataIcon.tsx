import type { DriverData } from '../../api/index.js';
import { dataKind } from '../driver-match.js';

/** A kind of collected data's icon, as the feature whose data it is declares it. */
export function DataIcon({ kind, size }: { kind: DriverData; size: number }) {
  const Icon = dataKind(kind)?.icon;
  return Icon ? <Icon size={size} aria-hidden="true" /> : null;
}
