import type { EmployeeTimecardPeriod } from './index.js';

// Timecard's addresses, which its prefetch builds before its page or client load: kept apart
// from the client, so the manifest every page loads carries only these.

/** Where its schedules are served, as core's schedule calls take it. */
export const schedules = '/api/dsp/schedules';
export const employeeTimecardUrl = (code: string, period?: EmployeeTimecardPeriod | null) =>
  `/api/dsp/employees/${encodeURIComponent(code)}${period ? `?from=${period.from}&to=${period.to}` : ''}`;
export const dailyTimecardsUrl = (date: string) =>
  `/api/dsp/timecards?date=${date}&sort=name&direction=asc`;
export const mealComparisonUrl = (date: string) =>
  `/api/dsp/paycom/meal-breaks?date=${encodeURIComponent(date)}`;
