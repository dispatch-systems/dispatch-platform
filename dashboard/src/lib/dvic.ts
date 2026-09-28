import type { DvicInspection, DvicInspections } from '../../../shared/contracts/dvic.js';
import { shiftDate } from './meal-breaks.js';

export const weekStart = (date: string) =>
  shiftDate(date, -new Date(date + 'T12:00:00Z').getUTCDay());
export const weekDays = (start: string) => Array.from({ length: 7 }, (_, i) => shiftDate(start, i));
export const inspectionDate = (date: string, weekday = false) =>
  new Intl.DateTimeFormat('en-US', {
    timeZone: 'UTC',
    month: 'short',
    day: 'numeric',
    ...(weekday ? { weekday: 'short' } : {}),
  }).format(new Date(date + 'T12:00:00Z'));

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
export const vehicleClass = (row: DvicInspection): VehicleClass =>
  row.fleetType === 'SV' ? 'dot' : 'non-dot';

export function filterInspections(rows: DvicInspection[], query: string, vehicles: VehicleClass) {
  const needle = query.trim().toLowerCase();
  return rows
    .filter(
      (row) =>
        (!vehicles || vehicleClass(row) === vehicles) &&
        [row.driverName, row.driverId, row.vin].some((value) =>
          value.toLowerCase().includes(needle),
        ),
    )
    .sort(
      (a, b) =>
        b.startDate.localeCompare(a.startDate) ||
        a.startTime.localeCompare(b.startTime) ||
        a.id.localeCompare(b.id),
    );
}

/** Publish a complete week at once, including every cursor page. Never show partial counts. */
export async function readInspectionWeek(
  read: (after: string | null) => Promise<DvicInspections>,
  signal: AbortSignal,
) {
  const rows = new Map<string, DvicInspection>();
  const cursors = new Set<string>();
  let cursor: string | null = null;
  for (let page = 0; page < 100; page++) {
    signal.throwIfAborted();
    const result = await read(cursor);
    signal.throwIfAborted();
    for (const row of result.inspections) rows.set(row.id, row);
    cursor = result.nextCursor;
    if (!cursor) return [...rows.values()];
    if (cursors.has(cursor))
      throw new Error('The inspection list changed while loading. Try again.');
    cursors.add(cursor);
  }
  throw new Error('This week is too large to display. Contact your platform administrator.');
}

export const fleetLabel = (fleet: string) =>
  fleet === 'CV' ? 'CV · Cargo Van' : fleet === 'SV' ? 'SV · Step Van' : fleet;

/** Share of the required minimum the inspection reached, capped at 1. */
export const inspectionShare = (row: DvicInspection) =>
  row.minimumSeconds > 0 ? Math.min(1, row.durationSeconds / row.minimumSeconds) : 0;

// Neutral bands by share of the minimum. The labels state the share, not a verdict.
export type Band = 'high' | 'mid' | 'low';
export const inspectionBand = (row: DvicInspection): Band =>
  inspectionShare(row) >= 0.75 ? 'high' : inspectionShare(row) >= 0.35 ? 'mid' : 'low';
export const bandLabel: Record<Band, string> = {
  high: '75%+ of minimum',
  mid: '35–75% of minimum',
  low: 'Under 35% of minimum',
};
export const bandRank: Record<Band, number> = { low: 0, mid: 1, high: 2 };

export const shortestFirst = (rows: DvicInspection[]) =>
  [...rows].sort(
    (a, b) =>
      inspectionShare(a) - inspectionShare(b) ||
      a.startTime.localeCompare(b.startTime) ||
      a.id.localeCompare(b.id),
  );

const initialsOf = (row: DvicInspection) =>
  (row.driverName || row.driverId)
    .split(' ')
    .slice(0, 2)
    .map((word) => word[0])
    .join('');
export const inspectionInitials = initialsOf;

export interface DriverWeek {
  driverId: string;
  driverName: string;
  fleets: string[];
  rows: DvicInspection[];
}

/** Drivers with the most short inspections first; a name change never splits a driver. */
export function driversByCount(rows: DvicInspection[]): DriverWeek[] {
  const map = new Map<string, DvicInspection[]>();
  for (const row of rows) map.set(row.driverId, [...(map.get(row.driverId) ?? []), row]);
  return [...map.entries()]
    .map(([driverId, list]) => ({
      driverId,
      driverName: list.at(-1)!.driverName || driverId,
      fleets: [...new Set(list.map((row) => row.fleetType))].sort(),
      rows: list,
    }))
    .sort(
      (a, b) =>
        b.rows.length - a.rows.length ||
        a.driverName.localeCompare(b.driverName) ||
        a.driverId.localeCompare(b.driverId),
    );
}

/** Names of the day's drivers with at least `minimum` short inspections in the loaded week. */
export function repeatDrivers(week: DvicInspection[], day: DvicInspection[], minimum = 3) {
  const present = new Set(day.map((row) => row.driverId));
  return driversByCount(week)
    .filter((driver) => present.has(driver.driverId) && driver.rows.length >= minimum)
    .map((driver) => driver.driverName);
}

/** Non-DOT vehicles (cargo vans and CDVs) first, then DOT (step vans); each with its minimum. */
export function groupByVehicleClass(rows: DvicInspection[]) {
  return (
    [
      { vehicles: 'non-dot', label: 'Non-DOT · Cargo Van & CDV' },
      { vehicles: 'dot', label: 'DOT · Step Van' },
    ] as const
  )
    .map((group) => ({
      ...group,
      rows: shortestFirst(rows.filter((row) => vehicleClass(row) === group.vehicles)),
    }))
    .filter((group) => group.rows.length)
    .map((group) => ({
      ...group,
      minimum: Math.max(...group.rows.map((row) => row.minimumSeconds)),
    }));
}
