import { useEffect, useMemo, useRef } from 'react';
import type { EmployeeTimecardPeriod } from '../../../api/index.js';
import { useMealPeriod } from '../../../api/client.js';
import {
  DataState,
  DataTable,
  useDataTable,
  type TableColumn,
} from '../../../../../core/shell/frontend/ui/index.js';
import { addDays } from '../../../../../core/shell/frontend/lib/calendar.js';
import { displayMeal } from '../../meal-breaks.js';
import { timecardDate } from '../../timecard-format.js';
import { mealColumns, mealLines, type MealLine } from '../meal-breaks/mealColumns.js';
import { MealGapSummary } from '../meal-breaks/MealGapSummary.js';
import '../meal-breaks/meal-breaks.css';

type Day = { id: string; date: string; line?: MealLine; status?: string };
const columns: TableColumn<Day>[] = [
  {
    id: 'date',
    header: 'Date',
    rowHeader: true,
    sticky: true,
    cell: (day) => (
      <div className="meal-employee">
        <span>{day.line?.index ? `Meal ${day.line.index + 1}` : timecardDate(day.date)}</span>
      </div>
    ),
  },
  ...mealColumns.slice(1).map((column): TableColumn<Day> => ({
    id: column.id,
    header: column.header,
    headerClassName: column.headerClassName,
    className: (day, context) =>
      typeof column.className === 'function'
        ? day.line
          ? column.className(day.line, context)
          : ''
        : (column.className ?? ''),
    cell: (day, context) =>
      day.line ? (
        column.cell(day.line, context)
      ) : column.id === 'comparison' ? (
        <span className="meal-status">{day.status}</span>
      ) : (
        '—'
      ),
  })),
];

export function EmployeeMeals({
  code,
  period,
  periodError,
  onRetryPeriod,
  today,
  refreshKey,
  focusDate,
}: {
  code: string;
  period?: EmployeeTimecardPeriod;
  periodError: string;
  onRetryPeriod: () => void;
  today: string;
  refreshKey: string;
  focusDate?: string;
}) {
  const request = useMealPeriod(`paycom:${code}`, period, refreshKey);
  const error = period ? request.error : periodError;
  const from = period?.from;
  const to = period?.to;
  const rows = useMemo(() => {
    if (!from || !to || !request.data) return [];
    const days: Day[] = [];
    const byDate = new Map(request.data.days.map((day) => [day.date, day]));
    for (let date = from; date <= to; date = addDays(date, 1)) {
      const day = byDate.get(date);
      const row = day?.rows[0];
      if (row) {
        const line = mealLines(row, displayMeal(row), '', date);
        for (const meal of [line, ...(line.more ?? [])])
          days.push({ id: `${date}:${meal.index}`, date, line: meal });
      } else
        days.push({
          id: date,
          date,
          status:
            date > today
              ? 'Upcoming'
              : day?.paycomCollectedAt && day.cortexPublications.length
                ? 'No activity'
                : 'Not collected',
        });
    }
    return days;
  }, [request.data, from, to, today]);
  const table = useDataTable({ columns, rows, rowId: (day) => day.id });
  const region = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (request.data && focusDate)
      region.current?.querySelector('.meal-focused-day')?.scrollIntoView({ block: 'nearest' });
  }, [request.data, focusDate]);
  return (
    <div className="employee-meals meal-page" aria-busy={!request.data && !error}>
      <div className="employee-meals-summary">
        {request.data && <MealGapSummary data={request.data} compact />}
      </div>
      <div className="employee-timecard-body">
        <div
          ref={region}
          className="table-wrap meal-table-scroll employee-meals-scroll"
          role="region"
          aria-label="Employee meal breaks"
          tabIndex={0}
        >
          <DataTable
            table={table}
            className="meal-table"
            label="Employee meal breaks"
            rowClassName={(day) =>
              [day.date === focusDate && 'meal-focused-day', day.line?.index && 'meal-extra']
                .filter(Boolean)
                .join(' ') || undefined
            }
          />
        </div>
        {!request.data && (
          <div className="employee-timecard-state">
            <DataState
              data={request.data}
              error={error}
              failed={!!error}
              retry={period ? request.refresh : onRetryPeriod}
            >
              {() => null}
            </DataState>
          </div>
        )}
      </div>
    </div>
  );
}
