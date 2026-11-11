import { WEEKDAYS } from './calendar.js';

/** The timing a weekly or monthly schedule edits; owners keep their own day and data fields. */
export type ScheduleTiming = {
  kind: 'times' | 'repeat';
  times: string[];
  every: number | null;
  from: string | null;
  to: string | null;
};

const FULL = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
export const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];
/** How often a repeat may run, in minutes: the server's half hour to twelve hours. */
export const EVERY = [30, 60, 90, 120, 180, 240, 360, 480, 720];
/** The most times one rule runs at. */
export const MAX_TIMES = 12;

/** How a rule is made: once a day, a few times a day, or repeating during hours. */
export type Pattern = 'once' | 'few' | 'repeat';
export const PATTERNS: { pattern: Pattern; label: string }[] = [
  { pattern: 'once', label: 'Once a day' },
  { pattern: 'few', label: 'A few times a day' },
  { pattern: 'repeat', label: 'Repeat during hours' },
];
export const patternOf = (rule: ScheduleTiming): Pattern =>
  rule.kind === 'repeat' ? 'repeat' : rule.times.length > 1 ? 'few' : 'once';

/** `rule` made as `pattern`, keeping what still fits it. */
export function withPattern<T extends ScheduleTiming>(rule: T, pattern: Pattern): T {
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
/** Whole timing, with set times unique and sorted. Other fields stay with the owner. */
export function finishedTiming<T extends ScheduleTiming>(rule: T): T | undefined {
  if (rule.kind === 'repeat')
    return rule.every && isTime(rule.from) && isTime(rule.to) ? rule : undefined;
  if (!rule.times.length || !rule.times.every(isTime)) return undefined;
  const times = [...new Set(rule.times)].sort();
  return times.length === rule.times.length ? { ...rule, times } : undefined;
}
export const isTime = (value: string | null): value is string =>
  !!value && /^\d{2}:\d{2}$/.test(value);

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
export const list = (items: string[]) =>
  items.length < 2 ? (items[0] ?? '') : `${items.slice(0, -1).join(', ')} and ${items.at(-1)}`;
/** "Every day", "Weekdays", "Weekends", "Mondays", "Mon–Thu", or "Mon, Wed, Fri". */
export function daysText(days: number[]) {
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
export function timingText(rule: ScheduleTiming, days: string) {
  if (rule.kind === 'times') {
    const times = rule.times.filter(isTime).map(clock);
    return times.length ? `${days} at ${list(times)}` : days;
  }
  if (!rule.every || !isTime(rule.from) || !isTime(rule.to)) return days;
  const window = rule.from === rule.to ? 'all day' : `${clock(rule.from)} to ${clock(rule.to)}`;
  return `${days}, every ${duration(rule.every)}, ${window}`;
}
