import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { cortexClock } from '../../frontend/meal-breaks.js';
import type {
  MealComparison,
  MealComparisonPage,
  MealSource,
  LateRule,
  EmployeeTimecard,
  Timecard,
} from '../../api/index.js';

export type MealComparisonSource = Omit<MealComparison, 'rows' | 'date'> & {
  date: string | null;
  rows: MealSource[];
};
let executable: string | undefined;
function assess(input: {
  date: string;
  late?: LateRule;
  rows?: MealSource[];
  timecards?: Timecard[];
}): { rows: MealComparison['rows']; timecards: EmployeeTimecard[] } {
  if (!executable && process.env.DISPATCH_ASSESSMENT_FIXTURE) {
    // test:ui names the copy it restored or built before starting browser workers.
    executable = process.env.DISPATCH_ASSESSMENT_FIXTURE;
  }
  if (!executable) {
    // Otherwise the test-only companion is the debug example built in this checkout.
    const metadata = JSON.parse(
      execFileSync('cargo', ['metadata', '--no-deps', '--format-version=1', '--locked'], {
        encoding: 'utf8',
      }),
    );
    executable = path.join(metadata.target_directory, 'debug/examples/assessment-fixture');
  }
  return JSON.parse(
    execFileSync(executable!, {
      input: JSON.stringify([input]),
      encoding: 'utf8',
      maxBuffer: 16 * 1024 * 1024,
    }),
  )[0];
}
export function assessMealResponse(
  source: MealComparisonSource,
  late: LateRule = { time: '10:01', departments: [] },
): MealComparison {
  if (!source.date) throw new Error('Meal fixtures need a date');
  return {
    ...source,
    date: source.date,
    rows: assess({ date: source.date, late, rows: source.rows }).rows,
  };
}
export function assessTimecards(cards: Timecard[]): EmployeeTimecard[] {
  return assess({ date: '2026-09-15', timecards: cards }).timecards;
}

/** A fake server page for browser controls; real page selection is guarded by API tests. */
export function mealPageFixture(source: MealComparisonSource, url: string): MealComparisonPage {
  const day = assessMealResponse(source);
  const q = new URL(url).searchParams;
  const counts = { all: day.rows.length, late: 0, different: 0, missing: 0, gaps: 0 };
  const filter = q.get('filter') ?? 'all';
  const search = (q.get('q') ?? '').toLowerCase();
  const sort = q.get('sort') ?? 'employee';
  const desc = q.get('direction') === 'desc';
  const statuses = [
    'cortex_unreadable',
    'different',
    'missing_lunch',
    'missing_data',
    'review_punches',
    'review_pairing',
    'no_flex_meal',
    'flex_only',
    'same',
  ];
  const numeric = (row: MealComparison['rows'][number]) => {
    const a = row.assessment,
      pair = a.pairs[0];
    switch (sort) {
      case 'inDay':
        return a.paycom.inDay?.minute;
      case 'outDay':
        return a.paycom.outDay?.minute;
      case 'outLunch':
        return pair?.outDifference == null ? undefined : Math.abs(pair.outDifference);
      case 'inLunch':
        return pair?.inDifference == null ? undefined : Math.abs(pair.inDifference);
      case 'comparison':
        return statuses.indexOf(a.status);
      default: {
        const meal = pair?.cortexIndex == null ? undefined : row.cortex[pair.cortexIndex];
        const instant = sort === 'lastDelivery' ? meal?.lastDelivery : meal?.firstDelivery;
        return instant && meal ? cortexClock(instant, day.date, meal.timezone)?.minute : undefined;
      }
    }
  };
  const rows = day.rows
    .filter((row) => {
      const a = row.assessment;
      counts.late += Number(a.lateIn);
      counts.different += Number(a.different);
      counts.missing += Number(a.missing);
      counts.gaps += Number(a.longGap);
      const flags = {
        all: true,
        late: a.lateIn,
        different: a.different,
        missing: a.missing,
        gaps: a.longGap,
      };
      return (
        flags[filter as keyof typeof flags] &&
        `${row.name} ${row.paycom?.employeeCode ?? ''} ${row.cortex.map((m) => m.driverName).join(' ')}`
          .toLowerCase()
          .includes(search)
      );
    })
    .sort((a, b) => {
      const order =
        sort === 'employee'
          ? a.name.localeCompare(b.name, 'en')
          : (numeric(a) ?? -Infinity) - (numeric(b) ?? -Infinity);
      return (desc ? -order : order) || 0;
    });
  const total = rows.length;
  const offset = Number(q.get('offset') ?? 0);
  const limit = q.get('limit') === 'all' ? total : Number(q.get('limit') ?? 100);
  return { ...day, rows: rows.slice(offset, offset + limit), total, counts };
}
