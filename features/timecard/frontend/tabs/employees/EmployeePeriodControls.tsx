import { ChevronLeft, ChevronRight } from 'lucide-react';
import type { EmployeeTimecardPeriod, EmployeeTimecardResponse } from '../../../api/index.js';
import { timecardPeriod } from '../../timecard-format.js';

export function EmployeePeriodControls({
  data,
  period,
  busy,
  onChange,
}: {
  data?: EmployeeTimecardResponse;
  period?: EmployeeTimecardPeriod | null;
  busy: boolean;
  onChange: (period: EmployeeTimecardPeriod) => void;
}) {
  const shown = period ?? data?.period;
  return (
    <nav className="employee-period-controls" aria-label="Timecard navigation">
      <button
        type="button"
        className="icon-button"
        aria-label="Previous timecard"
        disabled={!data?.previousPeriod}
        aria-disabled={busy || !data?.previousPeriod}
        onClick={() => !busy && data?.previousPeriod && onChange(data.previousPeriod)}
      >
        <ChevronLeft size={16} />
      </button>
      <span aria-live="polite">{shown ? timecardPeriod(shown.from, shown.to) : '—'}</span>
      <button
        type="button"
        className="icon-button"
        aria-label="Next timecard"
        disabled={!data?.nextPeriod}
        aria-disabled={busy || !data?.nextPeriod}
        onClick={() => !busy && data?.nextPeriod && onChange(data.nextPeriod)}
      >
        <ChevronRight size={16} />
      </button>
    </nav>
  );
}
