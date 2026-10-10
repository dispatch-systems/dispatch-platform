import { useEffect, useMemo, useRef } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
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

type Day = { id: string; date: string; line?: MealLine; status?: string; more?: Day[] };
const columns: TableColumn<Day>[] = [
  {
    id: 'date',
    header: 'Date',
    rowHeader: true,
    sticky: true,
    cell: (day, { expanded, toggle }) => (
      <div className="meal-employee">
        {day.line && day.line.index === 0 && (
          <button
            type="button"
            className="meal-expand"
            aria-expanded={expanded}
            aria-label={`Details for ${day.date}`}
            onClick={toggle}
          >
            {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          </button>
        )}
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
  today,
  refreshKey,
  focusDate,
}: {
  code: string;
  period: EmployeeTimecardPeriod;
  today: string;
  refreshKey: string;
  focusDate?: string;
}) {
  const request = useMealPeriod(`paycom:${code}`, period, refreshKey);
  const rows = useMemo(() => {
    const days: Day[] = [];
    const byDate = new Map(request.data?.days.map((day) => [day.date, day]));
    for (let date = period.from; date <= period.to; date = addDays(date, 1)) {
      const day = byDate.get(date);
      const row = day?.rows[0];
      if (row) {
        const line = mealLines(row, displayMeal(row), '', date);
        days.push({
          id: date,
          date,
          line,
          more: line.more?.map((line) => ({ id: `${date}:${line.index}`, date, line })),
        });
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
  }, [request.data, period.from, period.to, today]);
  const table = useDataTable({ columns, rows, rowId: (day) => day.id, subRows: (day) => day.more });
  const region = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (request.data && focusDate)
      region.current?.querySelector('.meal-focused-day')?.scrollIntoView({ block: 'nearest' });
  }, [request.data, focusDate]);
  return (
    <div className="employee-meals meal-page">
      <DataState data={request.data} error={request.error} failed={!!request.error}>
        {(data) => (
          <>
            <MealGapSummary data={data} compact />
            <div
              ref={region}
              className="table-wrap meal-table-scroll"
              role="region"
              aria-label="Employee meal breaks"
              tabIndex={0}
            >
              <DataTable
                table={table}
                className="meal-table"
                label="Employee meal breaks"
                rowClassName={(day) =>
                  day.date === focusDate
                    ? 'meal-focused-day'
                    : day.line?.index
                      ? 'meal-extra'
                      : undefined
                }
                renderDetail={(day) =>
                  day.line ? <MealGapSummary data={data} date={day.date} /> : null
                }
                detailClassName="meal-detail"
              />
            </div>
          </>
        )}
      </DataState>
    </div>
  );
}
