import type { BackupRuleInput, KeepUnit } from '../../../backups/api/index.js';
import { WEEKDAYS } from '../../../shell/frontend/lib/calendar.js';
import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';

// A DSP's backup rules as its Backups tab edits them: each on days of the week or dates of the
// month, at set times or every so often between two times, as the server keeps them
// (`backups/backend/when.rs`), and how long what each takes is kept, in the same words.

/** A rule being edited, by the key it's listed by: times still being typed may be empty. */
export type Draft = BackupRuleInput & { key: string };

export const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];
/** How often a repeat may run, in minutes: the server's half hour to twelve hours. */
export const EVERY = [30, 60, 90, 120, 180, 240, 360, 480, 720];
/** The most times one rule runs at. */
export const MAX_TIMES = 12;
export const UNITS: KeepUnit[] = ['day', 'week', 'month', 'year'];
const FULL = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

/** How a rule's day is made: once, a few times, or repeating during hours. */
export type Pattern = 'once' | 'few' | 'repeat';
export const PATTERNS: { pattern: Pattern; label: string }[] = [
  { pattern: 'once', label: 'Once a day' },
  { pattern: 'few', label: 'A few times a day' },
  { pattern: 'repeat', label: 'Repeat during hours' },
];
export const patternOf = (rule: Draft): Pattern =>
  rule.kind === 'repeat' ? 'repeat' : rule.times.length > 1 ? 'few' : 'once';

/** A new rule: once a day, every day, its time left to choose, kept 30 days, all but pictures. */
export const newRule = (key: string): Draft => ({
  key,
  id: null,
  on: 'week',
  days: EVERY_DAY,
  dates: [],
  lastDay: false,
  kind: 'times',
  times: [''],
  every: null,
  from: null,
  to: null,
  keep: 'for',
  keepCount: 30,
  keepUnit: 'day',
  skipped: ['pictures'],
});
/** `rule` made as `pattern`, keeping what still fits it. */
export function withPattern(rule: Draft, pattern: Pattern): Draft {
  if (pattern === 'repeat') {
    const from = rule.times.find(Boolean) ?? rule.from ?? '';
    return { ...rule, kind: 'repeat', times: [], every: rule.every ?? 60, from, to: rule.to ?? '' };
  }
  const first = rule.kind === 'repeat' ? (rule.from ?? '') : (rule.times[0] ?? '');
  const times = pattern === 'once' ? [first] : rule.kind === 'times' ? rule.times : [first];
  return {
    ...rule,
    kind: 'times',
    times: pattern === 'few' && times.length < 2 ? [...times, ''] : times,
    every: null,
    from: null,
    to: null,
  };
}
const isTime = (value: string | null): value is string => !!value && /^\d{2}:\d{2}$/.test(value);
/** A rule as the server keeps it, if it's whole: its days or dates chosen, its times set and in
 * order, each once, and how long it keeps set. */
export function finished(rule: Draft): BackupRuleInput | undefined {
  const { key: _key, ...input } = rule;
  if (rule.on === 'week' ? !rule.days.length : !rule.dates.length && !rule.lastDay)
    return undefined;
  if (rule.keep !== 'forever' && !(rule.keepCount && rule.keepCount >= 1)) return undefined;
  const sent = {
    ...input,
    days: rule.on === 'week' ? rule.days : [],
    dates: rule.on === 'month' ? [...rule.dates].sort((a, b) => a - b) : [],
    lastDay: rule.on === 'month' && rule.lastDay,
    keepCount: rule.keep === 'forever' ? null : rule.keepCount,
    keepUnit: rule.keep === 'for' ? (rule.keepUnit ?? 'day') : null,
  };
  if (rule.kind === 'repeat')
    return rule.every && isTime(rule.from) && isTime(rule.to) ? sent : undefined;
  if (!rule.times.length || !rule.times.every(isTime)) return undefined;
  const times = [...new Set(rule.times)].sort();
  return times.length === rule.times.length ? { ...sent, times } : undefined;
}

/** A time of day as a clock reads it: "6:00 AM". */
export function clock(time: string) {
  const hours = Number(time.slice(0, 2));
  return `${((hours + 11) % 12) + 1}:${time.slice(3)} ${hours < 12 ? 'AM' : 'PM'}`;
}
/** "30 min", "1 hr", "1 hr 30 min". */
export function duration(every: number) {
  const hours = Math.floor(every / 60);
  const rest = every % 60;
  if (!hours) return `${rest} min`;
  return rest ? `${hours} hr ${rest} min` : `${hours} hr`;
}
const list = (items: string[]) =>
  items.length < 2 ? (items[0] ?? '') : `${items.slice(0, -1).join(', ')} and ${items.at(-1)}`;
/** "1st", "2nd", "23rd", "11th". */
export function ordinal(date: number) {
  const tens = date % 100;
  if (tens >= 11 && tens <= 13) return `${date}th`;
  return `${date}${['th', 'st', 'nd', 'rd'][date % 10] ?? 'th'}`;
}
/** "Every day", "Weekdays", "Weekends", "Mondays", "Mon–Thu", or "Mon, Wed, Fri". */
function daysText(days: number[]) {
  const on = [...days].sort();
  const key = on.join('');
  if (key === '0123456') return 'Every day';
  if (key === '01234') return 'Weekdays';
  if (key === '56') return 'Weekends';
  const [first, last] = [on[0], on.at(-1)];
  if (first === undefined || last === undefined) return '';
  if (on.length === 1) return `${FULL[first]}s`;
  const running = on.every((day, at) => at === 0 || day === on[at - 1]! + 1);
  if (running && on.length > 2) return `${WEEKDAYS[first]}–${WEEKDAYS[last]}`;
  return on.map((day) => WEEKDAYS[day]).join(', ');
}
/** "1st and 15th of each month", "Last day of each month". */
function datesText(dates: number[], last: boolean) {
  const named = [...dates].sort((a, b) => a - b).map(ordinal);
  if (last) named.push(named.length ? 'last day' : 'Last day');
  return named.length ? `${list(named)} of each month` : '';
}
/** How a rule reads: "Weekdays at 6:00 AM and 2:00 PM", "1st and 15th of each month at 2:00 AM",
 * or "Every day, every 30 min, 6:00 AM to 9:00 PM". */
export function ruleText(rule: BackupRuleInput) {
  const days = rule.on === 'week' ? daysText(rule.days) : datesText(rule.dates, rule.lastDay);
  if (rule.kind === 'times') {
    const times = rule.times.filter(isTime).map(clock);
    return times.length ? `${days} at ${list(times)}` : days;
  }
  if (!rule.every || !isTime(rule.from) || !isTime(rule.to)) return days;
  const window = rule.from === rule.to ? 'all day' : `${clock(rule.from)} to ${clock(rule.to)}`;
  return `${days}, every ${duration(rule.every)}, ${window}`;
}
/** "days", or "day" for one. */
export const unitText = (unit: KeepUnit, count: number | null) =>
  `${unit}${count === 1 ? '' : 's'}`;
/** How long a rule keeps what it takes: "Keep forever", "Keep 30 days", "Keep last 12". */
export function keepText(rule: BackupRuleInput) {
  if (rule.keep === 'forever') return 'Keep forever';
  if (rule.keep === 'last') return `Keep last ${rule.keepCount ?? ''}`;
  return `Keep ${rule.keepCount ?? ''} ${unitText(rule.keepUnit ?? 'day', rule.keepCount)}`;
}

/** The day of the calendar, where the DSP is, `days` from now: 0 today, 1 tomorrow. */
const dayOf = (at: Date, timeZone: string) =>
  dateFormatter('en-CA', { timeZone, year: 'numeric', month: '2-digit', day: '2-digit' }).format(
    at,
  );
const daysTo = (at: Date, timeZone: string, now: Date) =>
  Math.round((Date.parse(dayOf(at, timeZone)) - Date.parse(dayOf(now, timeZone))) / 86_400_000);
/** When a backup is or was, where the DSP is: "Today, 7:45 AM", "Tomorrow, 2:00 AM",
 * "Wed, 9:00 AM" within a week, else "Oct 28, 9:00 AM". */
export function when(value: string, timeZone: string, now = new Date()) {
  const at = new Date(value);
  const clockText = dateFormatter('en-US', { timeZone, hour: 'numeric', minute: '2-digit' }).format(
    at,
  );
  const days = daysTo(at, timeZone, now);
  const named: Record<number, string> = { [-1]: 'Yesterday', 0: 'Today', 1: 'Tomorrow' };
  const day =
    named[days] ??
    dateFormatter('en-US', {
      timeZone,
      ...(Math.abs(days) < 7 ? { weekday: 'short' } : { month: 'short', day: 'numeric' }),
    }).format(at);
  return `${day}, ${clockText}`;
}
/** How long until a backup: "in 4 min", "in 6 hr", "in 3 days". */
export function until(value: string, timeZone: string, now = new Date()) {
  const at = new Date(value);
  const minutes = Math.round((at.getTime() - now.getTime()) / 60_000);
  if (minutes <= 0) return 'now';
  if (minutes < 60) return `in ${minutes} min`;
  if (minutes < 24 * 60) return `in ${Math.round(minutes / 60)} hr`;
  const days = Math.max(1, daysTo(at, timeZone, now));
  return `in ${days} day${days === 1 ? '' : 's'}`;
}
