import { performancePolicy } from '../../../../../core/shell/frontend/lib/performance-policy.js';
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { ArrowDownAZ, ArrowUpAZ, ChevronRight } from 'lucide-react';
import type { EmployeeTimecardPeriod, EmployeeTimecardResponse } from '../../../api/index.js';
import { useUpdateState } from '../../../../../core/shell/frontend/runtime/browser-update.js';
import { employeeTimecardUrl, useEmployeeTimecard, useEmployees } from '../../../api/client.js';
import {
  cancelPrefetches,
  prefetchData,
} from '../../../../../core/shell/frontend/runtime/prefetch.js';
import {
  DataState,
  Empty,
  Pagination,
  SearchInput,
} from '../../../../../core/shell/frontend/ui/index.js';
import { EmployeeAvatar } from './EmployeeAvatar.js';
import { EmployeeDetail } from './EmployeeDetail.js';
import './employees.css';

const statuses = ['all', 'active', 'inactive'] as const;
export function EmployeesPage({
  actions,
  refreshKey,
  selection,
  onSelectionChange: setSelection,
  employeeView,
  onViewChange,
  showMeals,
  today,
}: {
  actions: (timecard: EmployeeTimecardResponse | undefined) => ReactNode;
  refreshKey: string;
  selection?: { code: string; period: EmployeeTimecardPeriod | null; date?: string };
  onSelectionChange: (selection: {
    code: string;
    period: EmployeeTimecardPeriod | null;
    date?: string;
  }) => void;
  employeeView: 'timecard' | 'meals';
  onViewChange: (view: 'timecard' | 'meals') => void;
  showMeals: boolean;
  today: string;
}) {
  const [desc, setDesc] = useUpdateState('employee-sort-desc', false);
  const [query, setQuery] = useUpdateState('employee-query', '');
  const [status, setStatus] = useUpdateState<(typeof statuses)[number]>('employee-status', 'all');
  const [page, setPage] = useUpdateState('employee-page', 0);
  const [search, setSearch] = useState(query);
  useEffect(() => {
    if (search === query) return;
    const timer = setTimeout(() => {
      setSearch(query);
      setPage(0);
    }, performancePolicy.employeeSearchDelayMs);
    return () => clearTimeout(timer);
  }, [query, search, setPage]);
  const { data: current, stale, error } = useEmployees(search, status, desc, refreshKey, page);
  const data = current ?? stale;
  const busy = !current || search !== query;
  useEffect(() => {
    if (current && page && page * performancePolicy.employeePageSize >= current.total)
      setPage(Math.max(0, Math.ceil(current.total / performancePolicy.employeePageSize) - 1));
  }, [current, page, setPage]);
  const directoryEmployee =
    data?.employees.find((person) => person.code === selection?.code) ??
    (selection?.date ? undefined : data?.employees[0]);
  const code = selection?.date ? selection.code : (directoryEmployee?.code ?? '');
  const period = selection?.period ?? null;
  const timecard = useEmployeeTimecard(code, period, refreshKey, selection?.date);
  const shownPeriod =
    period ??
    timecard.data?.period ??
    (timecard.stale?.employee.code === code &&
    (!selection?.date ||
      (timecard.stale.period.from <= selection.date && timecard.stale.period.to >= selection.date))
      ? timecard.stale.period
      : null);
  const employee =
    directoryEmployee ??
    (timecard.data?.employee.code === code ? timecard.data.employee : undefined) ??
    (timecard.stale?.employee.code === code ? timecard.stale.employee : undefined);
  const preservePeriod = () => {
    if (code) setSelection({ code, period: shownPeriod });
  };
  const directory = useRef<HTMLUListElement>(null);
  const visibleCodes = data?.employees.map((person) => person.code).join(',');
  const from = shownPeriod?.from;
  const to = shownPeriod?.to;
  const prefetched = useMemo(() => (from && to ? { from, to } : undefined), [from, to]);
  useEffect(() => {
    const list = directory.current;
    if (!list) return;
    const owner = 'employees:directory';
    const observer = new IntersectionObserver(
      (entries) => {
        prefetchData(
          entries
            .filter((entry) => entry.isIntersecting)
            .map((entry) =>
              employeeTimecardUrl((entry.target as HTMLElement).dataset.employee!, prefetched),
            ),
          { owner },
        );
      },
      { root: list },
    );
    list.querySelectorAll('[data-employee]').forEach((button) => observer.observe(button));
    return () => {
      observer.disconnect();
      cancelPrefetches(owner);
    };
  }, [visibleCodes, prefetched]);
  return (
    <section className="employees-page" aria-label="Employees">
      <div className="employees-heading">
        <div className="employees-title">
          <h2>Employees</h2>
          <span className="employees-count" aria-label="Employee count">
            {data?.total ?? '…'}
          </span>
        </div>
        <div className="employees-sync">{actions(timecard.data)}</div>
      </div>
      <div className="employees-toolbar">
        <SearchInput
          type="search"
          label="Search employees"
          placeholder="Search employees…"
          value={query}
          onChange={(value) => {
            preservePeriod();
            setQuery(value);
          }}
        />
        <div className="employees-filters" role="group" aria-label="Employee status">
          {statuses.map((value) => (
            <button
              key={value}
              type="button"
              aria-pressed={status === value}
              onClick={() => {
                preservePeriod();
                setStatus(value);
                setPage(0);
              }}
            >
              {value === 'all' ? 'All' : value === 'active' ? 'Active' : 'Inactive'}
            </button>
          ))}
        </div>
        <button
          type="button"
          aria-label={desc ? 'Sort employees A to Z' : 'Sort employees Z to A'}
          onClick={() => {
            preservePeriod();
            setDesc(!desc);
            setPage(0);
          }}
        >
          {desc ? <ArrowUpAZ size={16} /> : <ArrowDownAZ size={16} />}
          {desc ? 'Z–A' : 'A–Z'}
        </button>
      </div>
      <DataState data={data} error={error} failed={!!error}>
        {(data) => (
          <>
            {code ? (
              <div className="employees-workspace" aria-busy={busy}>
                <nav className="employees-directory" aria-label="Employee directory">
                  <div className="employees-directory-heading">
                    <span>Employee</span>
                    <span>{desc ? 'Z–A' : 'A–Z'}</span>
                  </div>
                  <ul ref={directory} inert={busy}>
                    {data.employees.map((person) => (
                      <li key={person.code}>
                        <button
                          type="button"
                          className="employees-person"
                          data-employee={person.code}
                          aria-pressed={person.code === code}
                          onClick={() => setSelection({ code: person.code, period: shownPeriod })}
                          onPointerEnter={() =>
                            prefetchData([employeeTimecardUrl(person.code, shownPeriod)], {
                              owner: 'employees:directory',
                              priority: true,
                            })
                          }
                          onFocus={() =>
                            prefetchData([employeeTimecardUrl(person.code, shownPeriod)], {
                              owner: 'employees:directory',
                              priority: true,
                            })
                          }
                        >
                          <EmployeeAvatar employee={person} />
                          <span>{person.name}</span>
                          <ChevronRight size={16} aria-hidden="true" />
                        </button>
                      </li>
                    ))}
                  </ul>
                  <Pagination
                    page={page}
                    pageSize={performancePolicy.employeePageSize}
                    total={data.total}
                    onChange={(page) => {
                      preservePeriod();
                      setPage(page);
                    }}
                  />
                </nav>
                <DataState
                  data={employee}
                  error={employee ? '' : timecard.error}
                  failed={!!timecard.error}
                >
                  {(employee) => (
                    <EmployeeDetail
                      key={employee.code}
                      employee={employee}
                      period={period}
                      timecard={timecard}
                      onPeriodChange={(period) => setSelection({ code: employee.code, period })}
                      employeeView={employeeView}
                      onViewChange={onViewChange}
                      showMeals={showMeals}
                      today={today}
                      refreshKey={refreshKey}
                      focusDate={selection?.date}
                    />
                  )}
                </DataState>
              </div>
            ) : (
              <Empty
                title={
                  query || status !== 'all'
                    ? 'No employees match your search'
                    : 'No workforce data yet'
                }
              >
                {query || status !== 'all'
                  ? 'Try another name or status.'
                  : 'Employees will appear after the first collection finishes.'}
              </Empty>
            )}
          </>
        )}
      </DataState>
    </section>
  );
}
