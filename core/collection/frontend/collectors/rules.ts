import type { ScheduleCollection, ScheduleRule, ScheduleRuleInput } from '../../api/index.js';

// A collection's schedule rules as the Collectors page shows and edits them: each on chosen days
// of the week, Monday 0, at set times or every so often between two times, as the server keeps
// them (`collection/backend/rules.rs`), and the same words.

export const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
const FULL = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
export const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];
/** How often a repeat may run, in minutes: the server's half hour to twelve hours. */
export const EVERY = [30, 60, 90, 120, 180, 240, 360, 480, 720];
/** The most times one rule runs at. */
export const MAX_TIMES = 12;
const DAY = 24 * 60;

/** How a rule is made: once a day, a few times a day, or repeating during hours. */
export type Pattern = 'once' | 'few' | 'repeat';
export const PATTERNS: { pattern: Pattern; label: string }[] = [
  { pattern: 'once', label: 'Once a day' },
  { pattern: 'few', label: 'A few times a day' },
  { pattern: 'repeat', label: 'Repeat during hours' },
];
/** A rule being edited: what's sent, with times still being typed allowed empty. */
export type Draft = ScheduleRuleInput & { key: string };

export const patternOf = (rule: ScheduleRuleInput): Pattern =>
  rule.kind === 'repeat' ? 'repeat' : rule.times.length > 1 ? 'few' : 'once';

/** A new rule of `collects`: once a day, every day, its time left to choose. */
export const newRule = (collects: ScheduleCollection, key: string): Draft => ({
  key,
  id: null,
  collects,
  kind: 'times',
  days: EVERY_DAY,
  times: [''],
  every: null,
  from: null,
  to: null,
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
/** A rule as the server keeps it, if it's whole: its times set and in order, each once. */
export function finished(rule: Draft): ScheduleRuleInput | undefined {
  const { key: _key, ...input } = rule;
  if (!rule.days.length) return undefined;
  if (rule.kind === 'repeat')
    return rule.every && isTime(rule.from) && isTime(rule.to) ? input : undefined;
  if (!rule.times.length || !rule.times.every(isTime)) return undefined;
  const times = [...new Set(rule.times)].sort();
  return times.length === rule.times.length ? { ...input, times } : undefined;
}
const isTime = (value: string | null): value is string => !!value && /^\d{2}:\d{2}$/.test(value);
const minutes = (time: string) => Number(time.slice(0, 2)) * 60 + Number(time.slice(3));

/**
 * The minutes after the start of a day it runs on that a rule runs at, ascending: past a day's
 * length, on the next day. All day runs up to the next day's first, which that day runs.
 */
export function offsets(rule: ScheduleRuleInput): number[] {
  if (rule.kind === 'times') return rule.times.filter(isTime).map(minutes);
  if (!rule.every || !isTime(rule.from) || !isTime(rule.to)) return [];
  const start = minutes(rule.from);
  const length = (((minutes(rule.to) - start) % DAY) + DAY) % DAY;
  const last = length === 0 ? DAY - 1 : length;
  const runs: number[] = [];
  for (let at = 0; at <= last; at += rule.every) runs.push(start + at);
  return runs;
}
/** When each of `rules` runs on each day of a week, Monday first, in minutes from its start. */
export function week(rules: ScheduleRuleInput[]): number[][] {
  return DAYS.map((_, on) => {
    const runs = rules.flatMap((rule) =>
      rule.days.flatMap((day) =>
        offsets(rule)
          .filter((offset) => (day + Math.floor(offset / DAY)) % 7 === on)
          .map((offset) => offset % DAY),
      ),
    );
    return [...new Set(runs)].sort((a, b) => a - b);
  });
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
  if (running && on.length > 2) return `${DAYS[first]}–${DAYS[last]}`;
  return on.map((day) => DAYS[day]).join(', ');
}
/** How a rule reads: "Weekdays at 6:00 AM and 2:00 PM", or "Every day, every 30 min, 6:00 AM to 9:00 PM". */
export function ruleText(rule: ScheduleRuleInput | ScheduleRule) {
  const days = daysText(rule.days);
  if (rule.kind === 'times') {
    const times = rule.times.filter(isTime).map(clock);
    return times.length ? `${days} at ${list(times)}` : days;
  }
  if (!rule.every || !isTime(rule.from) || !isTime(rule.to)) return days;
  const window = rule.from === rule.to ? 'all day' : `${clock(rule.from)} to ${clock(rule.to)}`;
  return `${days}, every ${duration(rule.every)}, ${window}`;
}
