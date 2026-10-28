import type { Conversation, RouteDriver, Today, TodaysRoutes } from '../../api/index.js';
import type { Target } from '../conversations/target.js';
import { order } from '../inbox/inbox.js';

// Today's Routes as the page lists it: each driver on a route today, and each work phone nobody
// held that texted today, with its conversation as last read: the list's own (`inbox/inbox.ts`),
// which each update and the page's own writes keep, else Today's Routes' read. Those waiting
// first, the longest waiting first; then each wave by when it starts, its drivers by name; then
// the drivers with no wave; then the work phones nobody held, the latest written in first.

/** A row: a driver, with their work conversation once they have one, or a work phone nobody held. */
export type RouteRow = DriverRow | { kind: 'phone'; conversation: Conversation };
type DriverRow = { kind: 'driver'; driver: RouteDriver; conversation?: Conversation };

/** What a group of rows is: those waiting, a wave by when it starts, the drivers with no wave, or
 * the work phones nobody held. */
export type RouteHeading =
  { kind: 'waiting' } | { kind: 'wave'; at: number } | { kind: 'no_wave' } | { kind: 'no_driver' };
export type RouteGroup = { key: string; heading: RouteHeading; rows: RouteRow[] };

/** Today's Routes' rows, each conversation the latest of `listed` and the read's own. */
export function routeRows(routes: TodaysRoutes, listed: readonly Conversation[]): RouteRow[] {
  const latest = new Map(listed.map((conversation) => [conversation.id, conversation]));
  const fresh = (conversation: Conversation) => latest.get(conversation.id) ?? conversation;
  return [
    ...routes.drivers.map((driver): RouteRow => ({
      kind: 'driver',
      driver,
      conversation: driver.conversation ? fresh(driver.conversation) : undefined,
    })),
    ...routes.phones.map((phone): RouteRow => ({ kind: 'phone', conversation: fresh(phone) })),
  ];
}

/** The ID a row goes by: its conversation's, else its driver's. */
export const rowKey = (row: RouteRow) =>
  row.conversation?.id ?? (row.kind === 'driver' ? row.driver.driverId : '');
/** What a row opens: its conversation, or one started with the driver, to text them first. */
export const rowTarget = (row: RouteRow): Target =>
  row.kind === 'phone'
    ? { kind: 'conversation', id: row.conversation.id }
    : row.conversation
      ? { kind: 'conversation', id: row.conversation.id }
      : { kind: 'driver', driverId: row.driver.driverId };
/** Whether a row waits on an answer. */
export const waits = (row: RouteRow) => row.conversation?.state === 'waiting';

/** `rows` in their groups, in order, each group holding someone. */
export function routeGroups(rows: readonly RouteRow[]): RouteGroup[] {
  const waiting = rows.filter(waits).sort((a, b) => order(a.conversation!, b.conversation!));
  const rest = rows.filter((row) => !waits(row));
  const drivers = rest.filter((row): row is DriverRow => row.kind === 'driver');
  const waves = [...new Set(drivers.map((row) => row.driver.today.waveAt))]
    .filter((at): at is number => at !== null)
    .sort((a, b) => a - b);
  const phones = rest
    .filter((row) => row.kind === 'phone')
    .sort((a, b) => order(a.conversation, b.conversation));
  return [
    { key: 'waiting', heading: { kind: 'waiting' } as const, rows: waiting },
    ...waves.map((at) => ({
      key: `wave:${at}`,
      heading: { kind: 'wave', at } as const,
      rows: drivers.filter((row) => row.driver.today.waveAt === at),
    })),
    {
      key: 'no_wave',
      heading: { kind: 'no_wave' } as const,
      rows: drivers.filter((row) => row.driver.today.waveAt === null),
    },
    { key: 'no_driver', heading: { kind: 'no_driver' } as const, rows: phones },
  ].filter((group) => group.rows.length > 0);
}

/** A read as the page holds it: what it answered, and when it did, by the page's own clock
 * (`performance.now()`), so two reads can be told apart by which answered last. */
export type Answered<T> = { value: T; at: number };
/** Today's Routes as the page last read it. */
export type TodaysRead = Answered<TodaysRoutes>;

/**
 * A driver's route today and the work phone they hold now, one source for all that shows them:
 * whichever of two reads answered last, Today's Routes' (`todays`), as it's read every minute
 * while it shows, or their conversation's (`read`), as each change to it is. Today's Routes'
 * says none once the day is over, and the conversation's own stands for a driver it doesn't list.
 * A read failing, or another driver's change, leaves each as it last answered.
 */
export function driverToday(
  driverId: string | null,
  read: Answered<Today | null> | undefined,
  todays: TodaysRead | undefined,
): Today | null {
  const own = read?.value ?? null;
  if (!todays || (read && read.at > todays.at)) return own;
  if (todays.value.day === null) return null;
  return todays.value.drivers.find((each) => each.driverId === driverId)?.today ?? own;
}
