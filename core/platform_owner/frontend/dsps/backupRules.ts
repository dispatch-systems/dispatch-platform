import {
  EVERY_DAY,
  finishedTiming,
  daysText,
  timingText,
  list,
} from '../../../shell/frontend/lib/schedule-rules.js';
import type { BackupRuleInput, KeepUnit } from '../../../backups/api/index.js';
import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';

// A DSP's backup rules as its Backups tab edits them: each on days of the week or dates of the
// month, at set times or every so often between two times, as the server keeps them
// (`backups/backend/when.rs`), and how long what each takes is kept, in the same words.

/** A rule being edited, by the key it's listed by: times still being typed may be empty. */
export type Draft = BackupRuleInput & { key: string };

export const UNITS: KeepUnit[] = ['day', 'week', 'month', 'year'];
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
  return finishedTiming(sent);
}

/** "1st", "2nd", "23rd", "11th". */
export function ordinal(date: number) {
  const tens = date % 100;
  if (tens >= 11 && tens <= 13) return `${date}th`;
  return `${date}${['th', 'st', 'nd', 'rd'][date % 10] ?? 'th'}`;
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
  return timingText(rule, days);
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
