import type { MealPeriod } from '../../../api/index.js';
import { useMealPeriod } from '../../../api/client.js';
import { DataState } from '../../../../../core/shell/frontend/ui/index.js';
import { gapLabel } from '../../meal-breaks.js';
import { timecardDate, timecardPeriod } from '../../timecard-format.js';
import type { MealLine } from './mealColumns.js';

export function MealGapSummary({
  data,
  date,
  compact,
}: {
  data: MealPeriod;
  date?: string;
  compact?: boolean;
}) {
  const gaps = date ? data.gaps.filter((gap) => gap.date === date) : data.gaps;
  const excess = date
    ? gaps.reduce((sum, gap) => sum + gap.milliseconds - 300_000, 0)
    : data.excessMilliseconds;
  return (
    <section className="meal-gap-summary" aria-label="Meal break gap summary">
      <div className="meal-gap-summary-heading">
        <div>
          <strong>{gaps.length}</strong> {gaps.length === 1 ? 'gap' : 'gaps'} over 5 min
        </div>
        <div>
          Total time beyond 5 min <strong>{gapLabel(excess)}</strong>
        </div>
      </div>
      {!date && (
        <p className="muted">
          {timecardPeriod(data.period.from, data.period.to)} · Timecard period
        </p>
      )}
      {!compact &&
        (gaps.length ? (
          <table className="meal-gap-list">
            <thead>
              <tr>
                <th>Date</th>
                <th>Gap</th>
                <th>Duration</th>
                <th>Over 5 min</th>
              </tr>
            </thead>
            <tbody>
              {gaps.map((gap, index) => (
                <tr key={index}>
                  <td>{timecardDate(gap.date)}</td>
                  <td>
                    {gap.side === 'before' ? 'Before lunch' : 'After lunch'}
                    {gap.meal > 1 && ` · Meal ${gap.meal}`}
                  </td>
                  <td>{gapLabel(gap.milliseconds)}</td>
                  <td>{gapLabel(gap.milliseconds - 300_000)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="muted">No verified meal break gaps over 5 minutes.</p>
        ))}
    </section>
  );
}

export function MealDetail({ line, refreshKey }: { line: MealLine; refreshKey?: string | null }) {
  const request = useMealPeriod(line.row.id, line.date, refreshKey);
  return (
    <DataState data={request.data} error={request.error} failed={!!request.error}>
      {(data) => <MealGapSummary data={data} />}
    </DataState>
  );
}
