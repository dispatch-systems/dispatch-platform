import { dateFormatter } from './date-format.js';

// A DSP's days as `YYYY-MM-DD`: today in its timezone, and the days around it.

export function localDate(timezone: string, now = new Date()) {
  return dateFormatter('en-CA', {
    timeZone: timezone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(now);
}
export function shiftDate(date: string, days: number) {
  const d = new Date(`${date}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}
