import type { Conversation, RouteDriver, Today, TodaysRoutes } from '../../api/index.js';
import type { Target } from '../conversations/target.js';
import { latestFirst } from '../inbox/inbox.js';

// Today's Routes as the page lists it: each driver on a route today, and each work phone nobody
// held that texted today, with its conversation as last read: the list's own (`inbox/inbox.ts`),
// which each update and the page's own writes keep, else Today's Routes' read. The drivers A to
// Z, waiting or not; then the work phones nobody held, the latest written in first.

/** A row: a driver, with their work conversation once they have one, or a work phone nobody held. */
export type RouteRow = DriverRow | { kind: 'phone'; conversation: Conversation };
type DriverRow = { kind: 'driver'; driver: RouteDriver; conversation?: Conversation };

/** A group of rows: the drivers, or the work phones nobody held. */
export type RouteGroup = { key: 'drivers' | 'no_driver'; rows: RouteRow[] };

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

/** The conversations Today's Routes lists, as Recents lists them among every other section's. */
export const routeConversations = (groups: readonly RouteGroup[]) =>
  groups.flatMap((group) =>
    group.rows.flatMap((row) => (row.conversation ? [row.conversation] : [])),
  );
/** The route today of the driver each conversation Today's Routes lists is with, by its ID. */
export const routesToday = (groups: readonly RouteGroup[]) =>
  new Map(
    groups.flatMap((group) =>
      group.rows.flatMap((row) =>
        row.kind === 'driver' && row.conversation
          ? [[row.conversation.id, row.driver.today] as const]
          : [],
      ),
    ),
  );

/** `rows` in their groups, in order, each group holding someone: the drivers as the server lists
 * them, by name. */
export function routeGroups(rows: readonly RouteRow[]): RouteGroup[] {
  const drivers = rows.filter((row) => row.kind === 'driver');
  const phones = rows
    .filter((row) => row.kind === 'phone')
    .sort((a, b) => latestFirst(a.conversation, b.conversation));
  return [
    { key: 'drivers' as const, rows: drivers },
    { key: 'no_driver' as const, rows: phones },
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
 * while it shows, or their conversation's (`read`), as each change to it is. Once the day is
 * over, as Today's Routes lists none, the conversation's own stands, as it does for a driver Today's
 * Routes doesn't list: the work phone they had at the stop, for the two hours after, then none, each
 * as the server notes their conversation changed. A read failing, or another driver's change,
 * leaves each as it last answered.
 */
export function driverToday(
  driverId: string | null,
  read: Answered<Today | null> | undefined,
  todays: TodaysRead | undefined,
): Today | null {
  const own = read?.value ?? null;
  if (!todays || (read && read.at > todays.at)) return own;
  if (todays.value.day === null) return own;
  return todays.value.drivers.find((each) => each.driverId === driverId)?.today ?? own;
}
