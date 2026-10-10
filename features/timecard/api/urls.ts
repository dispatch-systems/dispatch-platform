import type { EmployeeTimecardPeriod } from './index.js';

// Timecard's addresses, which its prefetch builds before its page or client load: kept apart
// from the client, so the manifest every page loads carries only these.

/** Where its schedules are served, as core's schedule calls take it. */
export const schedules = '/api/dsp/schedules';
export const employeeTimecardUrl = (
  code: string,
  period?: EmployeeTimecardPeriod | null,
  date?: string,
) =>
  `/api/dsp/employees/${encodeURIComponent(code)}${period ? `?from=${period.from}&to=${period.to}` : date ? `?date=${date}` : ''}`;
export const dailyTimecardsUrl = (date: string) =>
  `/api/dsp/timecards?date=${date}&sort=name&direction=asc`;
export const mealComparisonUrl = (date: string) =>
  `/api/dsp/paycom/meal-breaks?date=${encodeURIComponent(date)}`;
export const mealPeriodUrl = (source: string, period: EmployeeTimecardPeriod | string) =>
  `/api/dsp/paycom/meal-breaks/period?source=${encodeURIComponent(source)}&${typeof period === 'string' ? `date=${period}` : `from=${period.from}&to=${period.to}`}`;
