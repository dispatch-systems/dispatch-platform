import type { EmployeeTimecardPeriod } from './index.js';

// Timecard's addresses, which its prefetch builds before its page or client load: kept apart
// from the client, so the manifest every page loads carries only these.

export const employeesPath = '/api/dsp/employees';
export const mealBreaksPath = '/api/dsp/paycom/meal-breaks';
export const mealPeriodPath = `${mealBreaksPath}/period`;
export const mealPeriodGapsPath = `${mealBreaksPath}/period-gaps`;
export const paycomSettingsPath = '/api/dsp/paycom/settings';
export const paycomStatusPath = '/api/dsp/paycom/status';
export const dailySyncPath = '/api/dsp/jobs/meal-breaks';
export const dailySyncUrl = (date: string) => `${dailySyncPath}?date=${date}`;
export const employeeTimecardUrl = (
  code: string,
  period?: EmployeeTimecardPeriod | null,
  date?: string,
) =>
  `${employeesPath}/${encodeURIComponent(code)}${period ? `?from=${period.from}&to=${period.to}` : date ? `?date=${date}` : ''}`;
export const dailyTimecardsUrl = (date: string, sort = 'name', desc = false) =>
  `/api/dsp/timecards?date=${date}&sort=${sort}&direction=${desc ? 'desc' : 'asc'}`;
export const mealPeriodGapsUrl = (date: string) =>
  `${mealPeriodGapsPath}?date=${encodeURIComponent(date)}`;
export const mealPeriodUrl = (source: string, period: EmployeeTimecardPeriod | string) =>
  `${mealPeriodPath}?source=${encodeURIComponent(source)}&${typeof period === 'string' ? `date=${period}` : `from=${period.from}&to=${period.to}`}`;

export const dailyPagePath = '/api/dsp/timecards/page';
export const mealPagePath = `${mealBreaksPath}/page`;
export const dailyPageUrl = (
  date: string,
  sort = 'name',
  desc = false,
  page = 0,
  limit: number | 'all' = 100,
) =>
  `${dailyPagePath}?date=${date}&sort=${sort}&direction=${desc ? 'desc' : 'asc'}&offset=${limit === 'all' ? 0 : page * limit}&limit=${limit}`;
export const mealPageUrl = (
  date: string,
  query = '',
  filter = 'all',
  sort = 'employee',
  desc = false,
  page = 0,
  limit: number | 'all' = 100,
) =>
  `${mealPagePath}?date=${date}&q=${encodeURIComponent(query)}&filter=${filter}&sort=${sort}&direction=${desc ? 'desc' : 'asc'}&offset=${limit === 'all' ? 0 : page * limit}&limit=${limit}`;
