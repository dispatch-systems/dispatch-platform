import type { EmployeeTimecardResponse, Timecard, PunchEvent } from '../../../api/index.js';
import { addDays } from '../../../../../core/shell/frontend/lib/calendar.js';
import {
  DataState,
  DataTable,
  Empty,
  useDataTable,
  type TableColumn,
} from '../../../../../core/shell/frontend/ui/index.js';
import { hoursAndMinutes, timecardDate, punchTime } from '../../timecard-format.js';

type EmployeeDay = Timecard & {
  events: PunchEvent[];
  regularMinutes: number | null;
  overtimeMinutes: number | null;
};
function periodDays(data: EmployeeTimecardResponse | undefined): EmployeeDay[] {
  if (!data?.collectedAt) return [];
  const byDate = new Map(data.timecards.map((card) => [card.date, card]));
  const days: EmployeeDay[] = [];
  for (let date = data.period.from; date <= data.period.to; date = addDays(date, 1)) {
    const card = byDate.get(date) ?? {
      employeeCode: data.employee.code,
      date,
      hours: 0,
      status: '',
      punches: [],
      assessment: { events: [] },
      regularMinutes: 0,
      overtimeMinutes: 0,
    };
    days.push({ ...card, events: card.assessment.events });
  }
  return days;
}
function punchCell(card: EmployeeDay, kind: string) {
  const punches = card.events.filter((event) => event.kind === kind);
  return punches.length
    ? punches.map((punch, index) => <div key={index}>{punchTime(punch.raw)}</div>)
    : '—';
}
const columns: TableColumn<EmployeeDay>[] = [
  { id: 'date', header: 'Date', cell: (card) => timecardDate(card.date) },
  { id: 'in', header: 'In', cell: (card) => punchCell(card, 'IN DAY') },
  { id: 'outLunch', header: 'Out lunch', cell: (card) => punchCell(card, 'OUT LUNCH') },
  { id: 'inLunch', header: 'In lunch', cell: (card) => punchCell(card, 'IN LUNCH') },
  { id: 'out', header: 'Out', cell: (card) => punchCell(card, 'OUT DAY') },
  {
    id: 'hours',
    header: 'Hours',
    cell: (card) =>
      card.regularMinutes === null || card.overtimeMinutes === null
        ? '—'
        : hoursAndMinutes((card.regularMinutes + card.overtimeMinutes) / 60),
  },
];

export function EmployeeTimecard({
  data,
  busy,
  error,
  onRetry,
}: {
  data: EmployeeTimecardResponse | undefined;
  busy: boolean;
  error: string;
  onRetry: () => void;
}) {
  const current = busy || error ? undefined : data;
  const days = periodDays(current);
  const recorded = days.filter(
    (card) => card.hours > 0 || card.punches.some((punch) => punch.in || punch.out),
  ).length;
  const table = useDataTable({ columns, rows: days, rowId: (card) => card.date });
  const incomplete = days.filter(
    (card) => card.regularMinutes === null || card.overtimeMinutes === null,
  ).length;
  const complete = days.filter(
    (card) => card.regularMinutes !== null && card.overtimeMinutes !== null,
  );
  const regular = complete.reduce((total, card) => total + (card.regularMinutes ?? 0), 0);
  const overtime = complete.reduce((total, card) => total + (card.overtimeMinutes ?? 0), 0);
  return (
    <>
      <div className="employee-timecard-body">
        <div
          className="table-wrap employee-timecard-scroll"
          role="region"
          aria-label="Timecard punches"
          tabIndex={0}
        >
          <DataTable table={table} className="employee-period-table" label="Employee timecard" />
        </div>
        {(!current || !days.length) && (
          <div className="employee-timecard-state">
            <DataState data={current} error={error} failed={!!error}>
              {() => (
                <Empty
                  title={
                    current?.collectedAt
                      ? 'No recorded activity in this timecard'
                      : 'This pay period has not been collected'
                  }
                />
              )}
            </DataState>
            {error && (
              <button type="button" onClick={onRetry}>
                Try again
              </button>
            )}
          </div>
        )}
      </div>
      <div className="employee-timecard-total">
        <div className="employee-timecard-days">
          <span>
            {current?.collectedAt ? recorded : '—'} recorded {recorded === 1 ? 'day' : 'days'}
          </span>
          {incomplete > 0 && <span>Incomplete days: {incomplete}</span>}
        </div>
        <div className="employee-hour-totals">
          <div>
            <span>Regular Hours:</span>
            <strong>{current?.collectedAt ? hoursAndMinutes(regular / 60) : '—'}</strong>
          </div>
          <div>
            <span>Overtime Hours:</span>
            <strong>{current?.collectedAt ? hoursAndMinutes(overtime / 60) : '—'}</strong>
          </div>
        </div>
      </div>
    </>
  );
}
