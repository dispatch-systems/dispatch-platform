import type { Employee, EmployeeTimecardPeriod } from '../../../api/index.js';
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
  const shown = data ?? (stale?.employee.code === employee.code ? stale : undefined);
  const person = shown?.employee ?? employee;
  const busy = !data && !error;
  let status = shown?.nextPeriod ? 'Previous timecard' : 'Latest';
  if (shown && !shown.collectedAt) status = 'Not collected';
  if (busy) status = 'Loading…';
  if (error) status = 'Unavailable';
  const shownPeriod = period ?? shown?.period;
  return (
    <section className="employee-detail" aria-label="Employee details">
      <div className="employee-detail-heading">
        <EmployeeAvatar name={person.name} />
        <Badge value={person.active ? 'active' : 'inactive'} />
      </div>
      <div className="employee-identity">
        <h3 title={person.name}>{person.name}</h3>
        <div className="employee-position" title={person.position}>
          {person.position || '\u00a0'}
        </div>
      </div>
      <div className="employee-timecard-section" aria-busy={!data && !error}>
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
        {employeeView === 'meals' && shownPeriod ? (
          <EmployeeMeals
            key={`${employee.code}:${shownPeriod.from}`}
            code={employee.code}
            period={shownPeriod}
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
