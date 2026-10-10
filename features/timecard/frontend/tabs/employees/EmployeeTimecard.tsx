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

type EmployeeDay = Timecard & { events: PunchEvent[] };
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
  { id: 'hours', header: 'Hours', cell: (card) => hoursAndMinutes(card.hours) },
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
  const minutes = days.reduce((total, card) => total + Math.round(card.hours * 60), 0);
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
        <span>
          {current?.collectedAt ? recorded : '—'} recorded {recorded === 1 ? 'day' : 'days'}
        </span>
        <div>
          <span>Total hours</span>
          <strong>{current?.collectedAt ? hoursAndMinutes(minutes / 60) : '—'}</strong>
        </div>
      </div>
    </>
  );
}
