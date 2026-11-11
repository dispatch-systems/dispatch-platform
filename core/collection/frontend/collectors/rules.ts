import {
  EVERY_DAY,
  finishedTiming,
  isTime,
  daysText,
  timingText,
} from '../../../shell/frontend/lib/schedule-rules.js';
import type { ScheduleCollection, ScheduleRule, ScheduleRuleInput } from '../../api/index.js';
import { WEEKDAYS } from '../../../shell/frontend/lib/calendar.js';

// A collection's schedule rules as the Collectors page shows and edits them: each on chosen days
// of the week, Monday 0, at set times or every so often between two times, as the server keeps
// them (`collection/backend/rules.rs`), and the same words.

/** A rule being edited: what's sent, with times still being typed allowed empty. */
export type Draft = ScheduleRuleInput & { key: string };

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
/** A rule as the server keeps it, if it's whole: its times set and in order, each once. */
export function finished(rule: Draft): ScheduleRuleInput | undefined {
  const { key: _key, ...input } = rule;
  if (!rule.days.length) return undefined;
  return finishedTiming(input);
}
const DAY = 24 * 60;
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
  return WEEKDAYS.map((_, on) => {
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

/** A rule in the same words as the server's activity log. */
export const ruleText = (rule: ScheduleRuleInput | ScheduleRule) =>
  timingText(rule, daysText(rule.days));
export { PATTERNS, patternOf, type Pattern } from '../../../shell/frontend/lib/schedule-rules.js';
