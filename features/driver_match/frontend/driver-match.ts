// What the Driver Match tab says about people, their IDs and the reasons two may be one.
import type {
  Driver,
  DriverData,
  DriverEvidence,
  DriverEvent,
  DriverId,
  DriverStatus,
} from '../api/index.js';
import { dateFormatter } from '../../../core/shell/frontend/lib/date-format.js';

export type DriverFilter = 'all' | 'matched' | 'review' | 'paycom' | 'amazon' | 'office' | 'former';
export const driverFilters: [DriverFilter, string][] = [
  ['all', 'All'],
  ['matched', 'Matched'],
  ['review', 'Needs review'],
  ['paycom', 'Paycom only'],
  ['amazon', 'Amazon only'],
  ['office', 'Office staff'],
  ['former', 'Former'],
];
const filterOf: Record<DriverStatus, DriverFilter> = {
  matched: 'matched',
  variant: 'matched',
  confirmed: 'matched',
  review: 'review',
  paycom_only: 'paycom',
  amazon_only: 'amazon',
  office: 'office',
  former: 'former',
};
export const driverFilter = (driver: Driver): DriverFilter => filterOf[driver.status];

export const statusLabels: Record<DriverStatus, string> = {
  matched: 'Name match',
  variant: 'Name variant',
  confirmed: 'Confirmed',
  review: 'Needs review',
  paycom_only: 'Paycom only',
  amazon_only: 'Amazon only',
  office: 'Office staff',
  former: 'Former',
};
/** The colour each status reads in: calm when matched, amber while waiting on a person. */
export const statusTones: Record<DriverStatus, string> = {
  matched: 'neutral',
  variant: 'neutral',
  confirmed: 'sage',
  review: 'warning',
  paycom_only: 'slate',
  amazon_only: 'slate',
  office: 'violet',
  former: 'neutral',
};

export const dataLabels: Record<DriverData, string> = {
  timecards: 'Timecards',
  routes: 'Routes',
  meal_breaks: 'Meal breaks',
  dvic: 'DVIC',
  daily_performance: 'Daily Performance',
  weekly_scorecard: 'Weekly Scorecard',
};
export const dataOrder: DriverData[] = [
  'timecards',
  'routes',
  'meal_breaks',
  'dvic',
  'weekly_scorecard',
  'daily_performance',
];
/** How much of one kind of data a person has, in its own unit. */
export function dataAmount(data: DriverData, count: number) {
  const unit = data === 'dvic' ? 'inspection' : data === 'weekly_scorecard' ? 'week' : 'day';
  return `${count.toLocaleString('en-US')} ${unit}${count === 1 ? '' : 's'}`;
}

/** One reason two people might be one, as a short sentence. */
export function evidenceText(evidence: DriverEvidence) {
  switch (evidence.kind) {
    case 'same_last_name':
      return 'Same last name';
    case 'same_first_name':
      return 'Same first name';
    case 'short_form':
      return `${evidence.short} is short for ${evidence.long}`;
    case 'last_initial':
      return 'Last initial matches';
    case 'worked_days':
      return evidence.same === evidence.of
        ? `Clocked in on all ${evidence.of} days they drove`
        : `Clocked in on ${evidence.same} of ${evidence.of} days they drove`;
  }
}
/** Days that disagree weaken a pair; every other reason supports it. */
export const evidenceSupports = (evidence: DriverEvidence) =>
  evidence.kind !== 'worked_days' || evidence.same === evidence.of;

/** When an ID was seen: one day, or the first and last. */
export const seenRange = (first: string | null, last: string | null, today: string) =>
  !first
    ? 'Not seen in collected data'
    : !last || first === last
      ? `Seen ${dayLabel(first, today)}`
      : `Seen ${dayLabel(first, today)} – ${dayLabel(last, today)}`;

/** "Today", "Yesterday" or the date, for a calendar day in the DSP's time zone. */
export function dayLabel(day: string | null | undefined, today: string) {
  if (!day) return '—';
  if (day === today) return 'Today';
  const yesterday = new Date(`${today}T00:00:00Z`);
  yesterday.setUTCDate(yesterday.getUTCDate() - 1);
  if (day === yesterday.toISOString().slice(0, 10)) return 'Yesterday';
  return dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    ...(day.slice(0, 4) === today.slice(0, 4) ? {} : { year: 'numeric' }),
    timeZone: 'UTC',
  }).format(new Date(`${day}T00:00:00Z`));
}

export const initials = (name: string) =>
  (name.match(/[\p{L}\p{N}]+/gu) ?? [])
    .slice(0, 2)
    .map((word) => [...word][0]!.toLocaleUpperCase('en-US'))
    .join('') || '?';
/** The same tone for a code everywhere it appears. */
export const toneOf = (code: string) => [...code].reduce((sum, c) => sum + c.charCodeAt(0), 0) % 4;
/** Amazon's transporter IDs run to fourteen characters; the start and end tell them apart. */
export const shortId = (id: DriverId) =>
  id.source === 'amazon' && id.id.length > 12 ? `${id.id.slice(0, 4)}…${id.id.slice(-4)}` : id.id;
export const firstId = (driver: Driver, source: DriverId['source']) =>
  driver.ids.find((id) => id.source === source);

/** The first name Amazon knows someone by, when it differs from the name shown. */
export function goesBy(driver: Driver) {
  const shown = driver.name.split(/\s+/)[0]?.toLocaleLowerCase('en-US');
  const amazon = firstId(driver, 'amazon')?.name.split(/\s+/)[0];
  return amazon && firstId(driver, 'paycom') && amazon.toLocaleLowerCase('en-US') !== shown
    ? amazon
    : undefined;
}

/** Whether a search finds someone: by any name a source writes, their code or an ID. */
export const driverSearchTerms = (driver: Driver) =>
  [driver.name, driver.code, ...driver.ids.flatMap((id) => [id.id, id.name])].map((text) =>
    text.toLocaleLowerCase('en-US'),
  );
export function driverMatches(driver: Driver, query: string) {
  const wanted = query.trim().toLocaleLowerCase('en-US');
  if (!wanted) return true;
  return driverSearchTerms(driver).some((text) => text.includes(wanted));
}

const sourceName = { paycom: 'Paycom', amazon: 'Amazon' };
/** One line of a person's history. */
export function eventText(event: DriverEvent) {
  const other = event.name ? `${event.name} (${event.code})` : event.code;
  switch (event.kind) {
    case 'added':
      return `Added from ${sourceName[event.source ?? 'paycom']}${event.name ? ` as ${event.name}` : ''}`;
    case 'linked':
      return event.link === 'name'
        ? `${sourceName[event.source ?? 'amazon']} ID joined by the same name`
        : event.link === 'variant'
          ? `${sourceName[event.source ?? 'amazon']} ID joined by a name variant`
          : event.link === 'saved'
            ? `${sourceName[event.source ?? 'amazon']} ID linked on the meal-break page`
            : `${sourceName[event.source ?? 'amazon']} ID confirmed as this person`;
    case 'merged':
      return `${event.code} merged into this person; that code now leads here`;
    case 'split':
      return `Split off from ${other}`;
    case 'apart':
      return `Marked as a different person from ${other}`;
  }
}
