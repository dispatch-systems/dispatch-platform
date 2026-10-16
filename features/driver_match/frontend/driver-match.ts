// What the Driver Match tab says about people, their IDs and the reasons two may be one.
import type {
  Driver,
  DriverData,
  DriverEvidence,
  DriverEvent,
  DriverId,
  DriverLink,
  DriverRole,
  DriverSource,
} from '../api/index.js';
import { dateFormatter } from '../../../core/shell/frontend/lib/date-format.js';
import { driverData, driverSources } from '../../../core/manifest/api/generated/driver-data.js';

/** Which people the tab shows: everyone still there, those of one role, or those a source lacks. */
export type PeopleFilter = 'all' | DriverRole | 'missing';
export const peopleFilters: [PeopleFilter, string][] = [
  ['all', 'All'],
  ['driver', 'Drivers'],
  ['helper', 'Helpers'],
  ['office', 'Office'],
  ['former', 'Former'],
  ['missing', 'Not in every system'],
];
/** Whether a source has an ID for someone. */
export const knows = (driver: Driver, source: DriverSource) =>
  driver.ids.some((id) => id.source === source);
/** Drivers and helpers belong in every source; anyone else only where they are. */
export const working = (driver: Driver) => driver.role === 'driver' || driver.role === 'helper';
/** Whether a person lacks an ID of a source the DSP has. */
export const missing = (driver: Driver, sources: readonly DriverSource[]) =>
  working(driver) && sources.some((source) => !knows(driver, source));

export const roleLabels: Record<DriverRole, string> = {
  driver: 'Driver',
  helper: 'Helper',
  office: 'Office',
  former: 'Former',
};
/** The colour each role reads in. */
export const roleTones: Record<DriverRole, string> = {
  driver: 'sage',
  helper: 'steel',
  office: 'violet',
  former: 'neutral',
};
export const linkLabels: Record<DriverLink, string> = {
  new: 'First ID',
  variant: 'Name variant',
  name: 'Same name',
  phone: 'Same phone',
  email: 'Same email',
  saved: 'Linked on Meal Breaks',
  person: 'Confirmed',
};
/** A link a match made, which someone may confirm. */
export const confirmable = (link: DriverLink) =>
  link === 'variant' || link === 'name' || link === 'phone' || link === 'email';

/** Every source of IDs, in their order, as their owners declare them. */
export const sourceKinds: readonly DriverSource[] = driverSources.map((source) => source.id);
/** How a source is named: "Paycom". */
export const sourceLabel = (source: DriverSource) =>
  driverSources.find((s) => s.id === source)?.label ?? source;

/** Every kind of collected data a person can appear in, in their order, as the features declare them. */
export const dataKinds: readonly DriverData[] = driverData.map((kind) => kind.id);
/** A kind of collected data as its feature declares it: its label, unit and icon. */
export const dataKind = (data: DriverData) => driverData.find((kind) => kind.id === data);
/** How a kind of collected data is named. */
export const dataLabel = (data: DriverData) => dataKind(data)?.label ?? data;
/** How much of one kind of data a person has, in its own unit. */
export function dataAmount(data: DriverData, count: number) {
  const unit = dataKind(data)?.unit ?? 'record';
  return `${count.toLocaleString('en-US')} ${unit}${count === 1 ? '' : 's'}`;
}

/** One reason two people might be one, as a short phrase. */
export function evidenceText(evidence: DriverEvidence) {
  switch (evidence.kind) {
    case 'same_email':
      return 'Same email';
    case 'same_phone':
      return 'Same phone';
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
        ? `Worked all ${evidence.of} days together`
        : `Worked ${evidence.same} of ${evidence.of} days together`;
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
  id.id.length > 12 ? `${id.id.slice(0, 4)}…${id.id.slice(-4)}` : id.id;
/** The address of a person's photo, at the version they have. */
export const photoUrl = (driver: Pick<Driver, 'code' | 'photo'>) =>
  driver.photo
    ? `/api/dsp/driver-match/drivers/${encodeURIComponent(driver.code)}/photo?v=${driver.photo.slice(0, 16)}`
    : undefined;

/**
 * The first name another source knows someone by, when it differs from the name shown, which
 * their first source's ID gives.
 */
export function goesBy(driver: Driver) {
  const shown = driver.name.split(/\s+/)[0]?.toLocaleLowerCase('en-US');
  const shownBy = driver.ids[0]?.source;
  for (const id of driver.ids.filter((id) => id.source !== shownBy)) {
    const given = id.name.split(/\s+/)[0];
    if (given && given.toLocaleLowerCase('en-US') !== shown)
      return { name: given, source: sourceLabel(id.source) };
  }
  return undefined;
}

/** A phone number as it is read in the US, `(612) 555-0103`; any other as written. */
export function phoneText(phone: string) {
  const parts = /^(?:\+?1)?(\d{3})(\d{3})(\d{4})$/.exec(phone.replace(/[\s().-]/g, ''));
  return parts ? `(${parts[1]}) ${parts[2]}-${parts[3]}` : phone;
}

/** Whether a search finds someone: by any name a source writes, their code, an ID or a contact. */
export const driverSearchTerms = (driver: Driver) =>
  [
    driver.name,
    driver.code,
    ...driver.ids.flatMap((id) => [
      id.id,
      id.name,
      ...id.emails,
      ...id.phones.flatMap((phone) => [phone, phoneText(phone)]),
    ]),
  ].map((text) => text.toLocaleLowerCase('en-US'));
export function driverMatches(driver: Driver, query: string) {
  const wanted = query.trim().toLocaleLowerCase('en-US');
  if (!wanted) return true;
  return driverSearchTerms(driver).some((text) => text.includes(wanted));
}

const linkedBy: Partial<Record<DriverLink, string>> = {
  name: 'joined by the same name',
  variant: 'joined by a name variant',
  email: 'joined by the same email',
  phone: 'joined by the same phone',
  saved: 'linked on the meal-break page',
  person: 'confirmed as this person',
};
/** One line of a person's history. */
export function eventText(event: DriverEvent) {
  const other = event.name ? `${event.name} (${event.code})` : event.code;
  const source = event.source ? sourceLabel(event.source) : 'An';
  switch (event.kind) {
    case 'added':
      return `Added from ${source}${event.name ? ` as ${event.name}` : ''}`;
    case 'linked':
      return `${source} ID ${linkedBy[event.link ?? 'person'] ?? 'linked'}`;
    case 'merged':
      return `${event.code} merged into this person; that code now leads here`;
    case 'split':
      return `Split off from ${other}`;
    case 'apart':
      return `Marked as a different person from ${other}`;
  }
}
