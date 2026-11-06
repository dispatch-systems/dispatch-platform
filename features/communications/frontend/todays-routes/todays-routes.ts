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
/** The route today of whom a conversation is with, while they drive one: a driver's work
 * conversation by its ID, else any conversation with someone Driver Match knows as a driver, by
 * their person's code; of a person's several, the route with the most stops. */
export function routeToday(groups: readonly RouteGroup[]) {
  const byConversation = new Map<string, Today>();
  const byPerson = new Map<string, Today>();
  for (const row of groups.flatMap((group) => group.rows)) {
    if (row.kind !== 'driver' || !onRoute(row.driver.today)) continue;
    const { today, person } = row.driver;
    if (row.conversation) byConversation.set(row.conversation.id, today);
    const kept = person.code === null ? undefined : byPerson.get(person.code);
    if (person.code !== null && (!kept || (today.stops ?? 0) > (kept.stops ?? 0)))
      byPerson.set(person.code, today);
  }
  return (conversation: Conversation): Today | undefined =>
    byConversation.get(conversation.id) ??
    (conversation.with.code === null ? undefined : byPerson.get(conversation.with.code));
}
/** Whether a driver has a route today, not only a work phone: one gone from the day's list has
 * none. */
const onRoute = (today: Today) => today.route !== null || today.stops !== null;

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

/** Whom a conversation is with today: their route and the work phone they hold now, and whether
 * they're working, on a route as live route data holds today. */
export type Day = { today: Today | null; working: boolean };

/** Of `drivers`, the one on the route with the most stops. */
const busiest = (drivers: readonly RouteDriver[]) =>
  drivers.reduce<RouteDriver | undefined>(
    (kept, each) => (!kept || (each.today.stops ?? 0) > (kept.today.stops ?? 0) ? each : kept),
    undefined,
  );

/**
 * Whom a conversation is with today, one source for all that shows it: whichever of two reads
 * answered last, Today's Routes' (`todays`), as it's read every minute while it shows, or their
 * conversation's (`read`), as each change to it is. While Today's Routes lists a day, they're
 * working as it has them on a route by any ID Driver Match knows them by, or by the driver a work
 * conversation is with. A work conversation shows the route and work phone it lists by its own
 * driver; another, their busiest route, with the work phone of the busiest of their IDs that has
 * one, as the server picks it; else the conversation's own stands. Once the day is over, as Today's
 * Routes lists none, no one is working, and the conversation's own route stands, as it does for
 * someone Today's Routes doesn't list: a driver's work phone they had at the stop, for the two
 * hours after, then none, each as the server notes their conversation changed. A read failing, or
 * another driver's change, leaves each as it last answered.
 */
export function dayOf(
  conversation: Conversation,
  read: Answered<Day> | undefined,
  todays: TodaysRead | undefined,
): Day {
  const own = read?.value ?? { today: null, working: false };
  if (!todays || (read && read.at > todays.at)) return own;
  if (todays.value.day === null) return { ...own, working: false };
  const { driverId, with: person } = conversation;
  const theirs = todays.value.drivers.filter(
    (each) =>
      each.driverId === driverId || (person.code !== null && each.person.code === person.code),
  );
  const working = theirs.some((each) => each.routed);
  if (driverId !== null) {
    const listed = theirs.find((each) => each.driverId === driverId);
    return { today: listed?.today ?? own.today, working };
  }
  const routed = busiest(theirs.filter((each) => each.routed));
  const phoned = busiest(theirs.filter((each) => each.today.phone !== null));
  const shown = routed ?? phoned;
  const today = shown && { ...shown.today, phone: phoned?.today.phone ?? null };
  return { today: today ?? own.today, working };
}
