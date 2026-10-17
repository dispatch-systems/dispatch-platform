// What the Driver Match tab says about people, their IDs and the reasons two may be one.
import type {
  DriverData,
  DriverSource,
  Evidence,
  IdLink,
  Person,
  PersonEvent,
  PersonId,
  PersonRole,
} from '../api/index.js';
import { personPath } from '../api/client.js';
import { dateFormatter } from '../../../core/shell/frontend/lib/date-format.js';
import { driverData, driverSources } from '../../../core/manifest/api/generated/driver-data.js';

/** How each role reads: its name, the name of its filter, and its colour. */
export const roles: Record<PersonRole, { label: string; filter: string; tone: string }> = {
  driver: { label: 'Driver', filter: 'Drivers', tone: 'sage' },
  helper: { label: 'Helper', filter: 'Helpers', tone: 'steel' },
  office: { label: 'Office', filter: 'Office', tone: 'violet' },
  former: { label: 'Former', filter: 'Former', tone: 'neutral' },
};
/** Which people the tab shows: everyone still there, those of one role, or those a source lacks. */
export type PeopleFilter = 'all' | PersonRole | 'missing';
export const peopleFilters: [PeopleFilter, string][] = [
  ['all', 'All'],
  ...Object.entries(roles).map(([role, { filter }]): [PeopleFilter, string] => [
    role as PersonRole,
    filter,
  ]),
  ['missing', 'Not in every system'],
];
/** Whether a source has an ID for someone. */
export const knows = (person: Person, source: DriverSource) =>
  person.ids.some((id) => id.source === source);
/** Drivers and helpers belong in every source; anyone else only where they are. */
export const working = (person: Person) => person.role === 'driver' || person.role === 'helper';
/** Whether a filter shows someone: someone who has left shows only under Former. */
export function shows(filter: PeopleFilter, person: Person, sources: readonly DriverSource[]) {
  switch (filter) {
    case 'all':
      return person.role !== 'former';
    case 'missing':
      return working(person) && sources.some((source) => !knows(person, source));
    default:
      return person.role === filter;
  }
}

/** How each way an ID came to its person reads: on its tag, and in their history. */
export const links: Record<IdLink, { label: string; joined: string }> = {
  new: { label: 'First ID', joined: 'added' },
  variant: { label: 'Name variant', joined: 'joined by a name variant' },
  name: { label: 'Same name', joined: 'joined by the same name' },
  phone: { label: 'Same phone', joined: 'joined by the same phone' },
  email: { label: 'Same email', joined: 'joined by the same email' },
  account: { label: 'Other account', joined: 'joined as another of their accounts' },
  person: { label: 'Confirmed', joined: 'confirmed as this person' },
};

/** Every source of IDs, in their order, as their owners declare them. */
export const sourceKinds: readonly DriverSource[] = driverSources.map((source) => source.id);
/** How a source is named: "Paycom". */
export const sourceLabel = (source: DriverSource) =>
  driverSources.find((s) => s.id === source)?.label ?? source;

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
export function evidenceText(evidence: Evidence) {
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

/** When an ID was seen: one day, or the first and last. */
export const seenRange = (first: string | null, last: string | null, today: string) =>
  !first
    ? 'Not seen in collected data'
    : !last || first === last
      ? `Seen ${day(first, today)}`
      : `Seen ${day(first, today)} – ${day(last, today)}`;

/** "Today", "Yesterday" or the date, for a calendar day in the DSP's time zone. */
function day(date: string, today: string) {
  if (date === today) return 'Today';
  const yesterday = new Date(`${today}T00:00:00Z`);
  yesterday.setUTCDate(yesterday.getUTCDate() - 1);
  if (date === yesterday.toISOString().slice(0, 10)) return 'Yesterday';
  return dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    ...(date.slice(0, 4) === today.slice(0, 4) ? {} : { year: 'numeric' }),
    timeZone: 'UTC',
  }).format(new Date(`${date}T00:00:00Z`));
}

/** The same tone for a code everywhere it appears. */
export const toneOf = (code: string) => [...code].reduce((sum, c) => sum + c.charCodeAt(0), 0) % 4;
/** Long IDs, as Amazon's transporter IDs, by their start and end, which tell them apart. */
export const shortId = (id: PersonId) =>
  id.id.length > 12 ? `${id.id.slice(0, 4)}…${id.id.slice(-4)}` : id.id;
/** The address of a person's photo, at the version they have. */
export const photoUrl = (person: Pick<Person, 'code' | 'photo'>) =>
  person.photo ? `${personPath(person.code)}/photo?v=${person.photo.slice(0, 16)}` : undefined;

/**
 * The first name another source knows someone by, when it differs from the name shown, which
 * their first source's ID gives.
 */
export function goesBy(person: Person) {
  const shown = person.name.split(/\s+/)[0]?.toLocaleLowerCase('en-US');
  const shownBy = person.ids[0]?.source;
  for (const id of person.ids.filter((id) => id.source !== shownBy)) {
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
/** A North American number's ten digits, as Driver Match compares phones; none for any other. */
export function phoneKey(phone: string) {
  const digits = phone.replace(/\D/g, '');
  const ten = digits.length === 11 && digits.startsWith('1') ? digits.slice(1) : digits;
  return ten.length === 10 ? ten : undefined;
}

/** What a search finds someone by: any name a source writes, their code, an ID or a contact. */
export const searchTerms = (person: Person) =>
  [
    person.name,
    person.code,
    ...person.ids.flatMap((id) => [
      id.id,
      id.name,
      ...id.emails,
      ...id.phones.flatMap((phone) => [phone, phoneText(phone)]),
    ]),
  ].map((text) => text.toLocaleLowerCase('en-US'));
/** Whether terms `searchTerms` gave find a search, as typed. */
export const found = (terms: string[], query: string) => {
  const wanted = query.trim().toLocaleLowerCase('en-US');
  return !wanted || terms.some((text) => text.includes(wanted));
};

/** One line of a person's history. */
export function eventText(event: PersonEvent) {
  const other = event.name ? `${event.name} (${event.code})` : event.code;
  const source = event.source ? sourceLabel(event.source) : 'An';
  switch (event.kind) {
    case 'added':
      return `Added from ${source}${event.name ? ` as ${event.name}` : ''}`;
    case 'linked':
      return `${source} ID ${links[event.link ?? 'person'].joined}`;
    case 'merged':
      return `${event.code} merged into this person; that code now leads here`;
    case 'split':
      return `Split off from ${other}`;
    case 'apart':
      return `Marked as a different person from ${other}`;
  }
}
