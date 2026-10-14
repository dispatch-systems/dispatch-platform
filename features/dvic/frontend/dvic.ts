import type { DvicCoverage, DvicInspection } from '../api/index.js';
import { shiftDate } from '../../../core/shell/frontend/lib/day.js';
import { dateFormatter } from '../../../core/shell/frontend/lib/date-format.js';

export const weekStart = (date: string) =>
  shiftDate(date, -new Date(date + 'T12:00:00Z').getUTCDay());
export const weekDays = (start: string) => Array.from({ length: 7 }, (_, i) => shiftDate(start, i));
/** Whether a checked report covers the day; if none does, its short inspections are unknown. */
export const isCollected = (covered: DvicCoverage[], date: string) =>
  covered.some((stretch) => stretch.from <= date && date <= stretch.through);
export const inspectionDate = (date: string, weekday = false) =>
  dateFormatter('en-US', {
    timeZone: 'UTC',
    month: 'short',
    day: 'numeric',
    ...(weekday ? { weekday: 'short' } : {}),
  }).format(new Date(date + 'T12:00:00Z'));
export const inspectionWeekday = (date: string) =>
  dateFormatter('en-US', { timeZone: 'UTC', weekday: 'short' }).format(
    new Date(date + 'T12:00:00Z'),
  );

// Amazon supplies wall times without an offset. Never pass them through the viewer's zone.
export function inspectionClock(value: string) {
  const match = /[ T](\d{2}):(\d{2}):(\d{2})/.exec(value);
  if (!match) return value;
  const hour = Number(match[1]);
  return `${hour % 12 || 12}:${match[2]}:${match[3]} ${hour < 12 ? 'AM' : 'PM'}`;
}

// Whole seconds, rounded down, so an inspection just below 90s never reads as 90s. The
// epsilon only absorbs floating-point noise such as 19.999999999999996.
export function inspectionDuration(seconds: number) {
  if (seconds > 0 && seconds < 1) return '<1s';
  const value = Math.floor(seconds + 1e-6);
  const minutes = Math.floor(value / 60);
  const remainder = value - minutes * 60;
  return minutes ? `${minutes}m${remainder ? ` ${remainder}s` : ''}` : `${remainder}s`;
}
// The shortfall rounds up, so a duration and its shortfall always add back to the minimum.
export const inspectionShortfall = (seconds: number) =>
  inspectionDuration(Math.max(0, Math.ceil(seconds - 1e-6)));

/** Step vans are DOT vehicles with the 5-minute minimum; cargo vans and CDVs are not. */
export type VehicleClass = '' | 'dot' | 'non-dot';

export const fleetLabel = (fleet: string) =>
  fleet === 'CV' ? 'CV · Cargo Van' : fleet === 'SV' ? 'SV · Step Van' : fleet;

/** Share of the required minimum the inspection reached, capped at 1. */
export const inspectionShare = (row: Pick<DvicInspection, 'durationSeconds' | 'minimumSeconds'>) =>
  row.minimumSeconds > 0 ? Math.min(1, row.durationSeconds / row.minimumSeconds) : 0;

// Neutral bands by share of the minimum. The labels state the share, not a verdict.
export type Band = 'high' | 'mid' | 'low';
export const inspectionBand = (
  row: Pick<DvicInspection, 'durationSeconds' | 'minimumSeconds'>,
): Band => (inspectionShare(row) >= 0.75 ? 'high' : inspectionShare(row) >= 0.35 ? 'mid' : 'low');
export const bandLabel: Record<Band, string> = {
  high: '75%+ of minimum',
  mid: '35–75% of minimum',
  low: 'Under 35% of minimum',
};

const initialsOf = (row: DvicInspection) =>
  (row.driverName || row.driverId)
    .split(' ')
    .slice(0, 2)
    .map((word) => word[0])
    .join('');
export const inspectionInitials = initialsOf;
