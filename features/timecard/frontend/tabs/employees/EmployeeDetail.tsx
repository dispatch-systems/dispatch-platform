import type {
  Employee,
  EmployeeTimecardPeriod,
  EmployeeTimecardResponse,
} from '../../../api/index.js';
import type { useEmployeeTimecard } from '../../../api/client.js';
import { Badge } from '../../../../../core/shell/frontend/ui/index.js';
import { EmployeeAvatar } from './EmployeeAvatar.js';
import { EmployeeTimecard } from './EmployeeTimecard.js';
import { EmployeePeriodControls } from './EmployeePeriodControls.js';
import { EmployeeMeals } from './EmployeeMeals.js';

export function EmployeeDetail({
  employee,
  period,
  onPeriodChange,
  timecard,
  employeeView,
  onViewChange,
  showMeals,
  today,
  refreshKey,
  focusDate,
}: {
  employee: Employee;
  period: EmployeeTimecardPeriod | null;
  onPeriodChange: (period: EmployeeTimecardPeriod) => void;
  timecard: ReturnType<typeof useEmployeeTimecard>;
  employeeView: 'timecard' | 'meals';
  onViewChange: (view: 'timecard' | 'meals') => void;
  showMeals: boolean;
  today: string;
  refreshKey: string;
  focusDate?: string;
}) {
  const { data, stale, error, refresh } = timecard;
  const matches = (value: EmployeeTimecardResponse | undefined) =>
    value?.employee.code === employee.code &&
    (!period || (value.period.from === period.from && value.period.to === period.to)) &&
    (!focusDate || (value.period.from <= focusDate && value.period.to >= focusDate));
  const current = matches(data) ? data : undefined;
  const shown = current ?? (matches(stale) ? stale : undefined);
  const person = shown?.employee ?? employee;
  const busy = !current && !error;
  let status = shown?.nextPeriod ? 'Previous timecard' : 'Latest';
  if (shown && !shown.collectedAt) status = 'Not collected';
  if (busy) status = 'Loading…';
  if (error) status = 'Unavailable';
  const shownPeriod = period ?? shown?.period;
  return (
    <section className="employee-detail" aria-label="Employee details">
      <div className="employee-detail-heading">
        <EmployeeAvatar employee={person} />
        <div className="employee-identity">
          <h3 title={person.name}>{person.name}</h3>
          <div className="employee-position" title={person.position}>
            {person.position || '\u00a0'}
          </div>
        </div>
        <Badge value={person.active ? 'active' : 'inactive'} />
      </div>
      <div className="employee-timecard-section" aria-busy={busy}>
        <div className="employee-timecard-heading">
          <div className="employees-filters" role="group" aria-label="Employee view">
            <button
              type="button"
              aria-pressed={employeeView === 'timecard'}
              onClick={() => onViewChange('timecard')}
            >
              Timecard
            </button>
            {showMeals && (
              <button
                type="button"
                aria-pressed={employeeView === 'meals'}
                onClick={() => onViewChange('meals')}
              >
                Meal Breaks
              </button>
            )}
          </div>
          <span className={!shown?.collectedAt || shown.nextPeriod ? 'muted' : 'employee-latest'}>
            {status}
          </span>
        </div>
        <EmployeePeriodControls
          data={shown}
          period={period}
          busy={busy}
          onChange={onPeriodChange}
        />
        {employeeView === 'meals' ? (
          <EmployeeMeals
            code={employee.code}
            period={shownPeriod}
            periodError={shownPeriod ? '' : error}
            onRetryPeriod={refresh}
            today={today}
            refreshKey={refreshKey}
            focusDate={focusDate}
          />
        ) : (
          <EmployeeTimecard data={shown} busy={busy} error={error} onRetry={refresh} />
        )}
      </div>
    </section>
  );
}
