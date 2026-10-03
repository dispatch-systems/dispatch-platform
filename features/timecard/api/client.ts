import { useEffect } from 'react';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { api, useCachedData, useData } from '../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { cancelPrefetches, prefetchData } from '../../../core/shell/frontend/runtime/prefetch.js';
import type {
  DailyTimecards,
  EmployeeTimecardPeriod,
  EmployeeTimecardResponse,
  EmployeesResponse,
  Job,
} from '../../../shared/contracts/index.js';
import type { MealComparison } from '../../../shared/contracts/meals.js';
import type { PaycomPreferences, PaycomSettings } from '../../../shared/contracts/paycom.js';

// Timecard's endpoints, as its pages call them.

export const employeeTimecardUrl = (code: string, period?: EmployeeTimecardPeriod | null) =>
  `/api/dsp/employees/${encodeURIComponent(code)}${period ? `?from=${period.from}&to=${period.to}` : ''}`;
export const syncEmployeeTimecard = (
  code: string,
  period: EmployeeTimecardPeriod,
  requestId: string,
) => api<Job>(`/api/dsp/employees/${encodeURIComponent(code)}/sync`, { requestId, ...period });
export const useEmployeeTimecard = (
  code: string,
  period: EmployeeTimecardPeriod | null,
  refreshKey: string,
) => {
  const url = code ? employeeTimecardUrl(code, period) : '';
  const result = useCachedData<EmployeeTimecardResponse>(url, 0, refreshKey);
  useEffect(() => {
    if (!result.data) return;
    dataCache.alias(url, employeeTimecardUrl(code, result.data.period));
    if (!result.data.nextPeriod) dataCache.alias(url, employeeTimecardUrl(code));
  }, [url, code, result.data]);
  const previous = result.data?.previousPeriod;
  const next = result.data?.nextPeriod;
  useEffect(() => {
    if (!code) return;
    const owner = `periods:${code}`;
    prefetchData(
      [previous, next]
        .filter((period) => period != null)
        .map((period) => employeeTimecardUrl(code, period)),
      { owner },
    );
    return () => cancelPrefetches(owner);
  }, [code, previous?.from, previous?.to, next?.from, next?.to]);
  return result;
};

export const useEmployees = (
  query: string,
  status: string,
  descending: boolean,
  refreshKey: string,
  page = 0,
) =>
  useCachedData<EmployeesResponse>(
    `/api/dsp/employees?q=${encodeURIComponent(query)}&status=${status}&limit=${performancePolicy.employeePageSize}&offset=${page * performancePolicy.employeePageSize}&direction=${descending ? 'desc' : 'asc'}`,
    0,
    refreshKey,
  );
export const dailyTimecardsUrl = (date: string) =>
  `/api/dsp/timecards?date=${date}&sort=name&direction=asc`;
export const useDailyTimecards = (date: string, refreshKey?: string | null) =>
  useCachedData<DailyTimecards>(
    dailyTimecardsUrl(date),
    performancePolicy.recoveryPollMs,
    refreshKey,
  );
export const mealComparisonUrl = (date: string) =>
  `/api/dsp/paycom/meal-breaks?date=${encodeURIComponent(date)}`;
export const useMealComparison = (date: string, refreshKey?: string | null) =>
  useCachedData<MealComparison>(
    mealComparisonUrl(date),
    performancePolicy.recoveryPollMs,
    refreshKey,
  );

const paycomSettings = '/api/dsp/paycom/settings';
/** Editors use a fresh DSP-scoped read; the timecard view shares its session cache. */
export const usePaycomSettings = (dspId?: string) =>
  useData<PaycomSettings>(
    paycomSettings,
    120000,
    dspId,
    dspId ?? paycomSettings,
    dspId === undefined,
  );
export const savePaycomSettings = (revision: number, values: PaycomPreferences) =>
  api<PaycomSettings>(paycomSettings, { revision, values });
