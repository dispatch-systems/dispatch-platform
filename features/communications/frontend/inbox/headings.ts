import { dayLabel, dayOf } from '../shared/format.js';
import type { Row } from './inbox.js';
import type { Listing } from './listing.js';

// The headings a list's rows are under: those waiting, then those answered, under Responded, or
// in Recents under the day each was last written in; and Recents' split at Previous days.

/** A list's rows under their headings, with how many people wait under Waiting. */
export type Headed = { key: string; label: string; count?: number; rows: Row[] };

/** Those waiting, then those answered, under the headings `answered` gives them. */
export function byState(
  shown: readonly Row[],
  waiting: number,
  answered: (rows: Row[]) => Headed[],
): Headed[] {
  const headed = [
    {
      key: 'waiting',
      label: 'Waiting',
      count: waiting,
      rows: shown.filter((each) => each.row.state === 'waiting'),
    },
    ...answered(shown.filter((each) => each.row.state !== 'waiting')),
  ];
  return headed.filter((each) => each.rows.length > 0);
}

/** Those answered, under Responded. */
export const responded = (rows: Row[]): Headed[] => [
  { key: 'responded', label: 'Responded', rows },
];

/** Under the day each was last written in: Today, Yesterday, then by the day. */
export function byDay(shown: readonly Row[], { timeZone, now }: Pick<Listing, 'timeZone' | 'now'>) {
  const headed: Headed[] = [];
  for (const each of shown) {
    const at = each.row.lastAt ?? now;
    const key = dayOf(at, timeZone);
    const last = headed.at(-1);
    if (last?.key === key) last.rows.push(each);
    else headed.push({ key, label: dayLabel(at, timeZone, now), rows: [each] });
  }
  return headed;
}

/** Recents' headings split at Previous days: those waiting and today's above it, and the days
 * before below it, every row while they're `shown`, else only what's `open`. */
export function atPreviousDays(
  headed: readonly Headed[],
  today: string,
  shown: boolean,
  open?: string,
) {
  const split = headed.findIndex((each) => each.key !== 'waiting' && each.key !== today);
  const previous = split < 0 ? [] : headed.slice(split);
  return {
    current: split < 0 ? headed : headed.slice(0, split),
    previous: shown
      ? previous
      : previous
          .map((each) => ({ ...each, rows: each.rows.filter(({ row }) => row.id === open) }))
          .filter((each) => each.rows.length > 0),
    /** Whether there are days before today to show. */
    any: previous.length > 0,
  };
}
