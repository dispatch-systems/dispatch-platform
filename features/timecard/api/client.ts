import { useEffect } from 'react';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { useCachedData, useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { cancelPrefetches, prefetchData } from '../../../core/shell/frontend/runtime/prefetch.js';
import type {
  DailyTimecards,
  DailyTimecard,
  DailyTimecardPage,
  MealComparisonPage,
  EmployeeTimecardPeriod,
  EmployeeTimecardResponse,
  EmployeeSyncStatus,
  EmployeesResponse,
  MealPeriod,
  MealPeriodGaps,
  PaycomPreferences,
  PaycomSettings,
} from './index.js';
import type { ScheduleInput } from '../../../core/collection/api/index.js';
import {
  dailyTimecardsUrl,
  dailyPageUrl,
  mealPageUrl,
  employeeTimecardUrl,
  mealComparisonUrl,
  mealPeriodGapsUrl,
  mealPeriodUrl,
  schedules,
} from './urls.js';

export {
  dailyPageUrl,
  mealPageUrl,
  dailyTimecardsUrl,
  employeeTimecardUrl,
  mealComparisonUrl,
  schedules,
};

// Timecard's endpoints, as its pages call them. Only its own screens raise these, so their
// words load with them.
const api = wordedApi({
  meal_sync_paycom_required: 'Connect Paycom before syncing meal breaks.',
  meal_sync_flex_required: 'Connect Cortex in Settings → Connections before syncing Flex.',
  meal_sync_scope_required: 'Complete your DSP profile with a station code to sync Flex.',
  settings_changed_reload_before_saving:
    'These settings changed in another session. Discard your draft and try again.',
});

/** Syncs a day's meal breaks from both their sources, once for `requestId`. */
export const syncMealBreaks = (date: string, requestId: string) =>
  api('/api/dsp/jobs/meal-breaks', { requestId, date });
/** When a schedule with this timing would run next. */
export const previewSchedule = (
  timing: Pick<ScheduleInput, 'cadence' | 'intervalMinutes' | 'localTime'> & {
    scheduleId?: string;
  },
  signal: AbortSignal,
) => api<{ nextRun: string }>(`${schedules}/preview`, timing, signal);
export const syncEmployeePeriod = (period: EmployeeTimecardPeriod, requestId: string) =>
  api<EmployeeSyncStatus>('/api/dsp/jobs/employees', { requestId, ...period });
export const useEmployeeTimecard = (
  code: string,
  period: EmployeeTimecardPeriod | null,
  refreshKey: string,
  date?: string,
) => {
  const url = code ? employeeTimecardUrl(code, period, date) : '';
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
export const useDailyTimecards = (url: string, refreshKey?: string | null) =>
  useCachedData<DailyTimecardPage>(url, performancePolicy.recoveryPollMs, refreshKey);
export const useMealComparison = (url: string, refreshKey?: string | null) =>
  useCachedData<MealComparisonPage>(url, performancePolicy.recoveryPollMs, refreshKey);
export const useDailyPunches = (date: string, code: string, refreshKey?: string | null) =>
  useCachedData<DailyTimecard>(
    `/api/dsp/timecards/punches/${encodeURIComponent(code)}?date=${date}`,
    performancePolicy.recoveryPollMs,
    refreshKey,
  );
export const exportDailyTimecards = (date: string, sort: string, desc: boolean) =>
  api<DailyTimecards>(
    dailyTimecardsUrl(date).replace(
      'sort=name&direction=asc',
      `sort=${sort}&direction=${desc ? 'desc' : 'asc'}`,
    ),
  );
export const exportMealComparison = (
  date: string,
  query: string,
  filter: string,
  sort: string,
  desc: boolean,
) => api<MealComparisonPage>(mealPageUrl(date, query, filter, sort, desc, 0, 'all'));
export const useMealPeriod = (
  source: string,
  period: EmployeeTimecardPeriod | string | undefined,
  refreshKey?: string | null,
) =>
  useCachedData<MealPeriod>(
    period ? mealPeriodUrl(source, period) : '',
    performancePolicy.recoveryPollMs,
    refreshKey,
  );
/** Everyone's count of verified gaps over the timecard period holding `date`. */
export const useMealPeriodGaps = (date: string, refreshKey?: string | null) =>
  useCachedData<MealPeriodGaps>(
    mealPeriodGapsUrl(date),
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
