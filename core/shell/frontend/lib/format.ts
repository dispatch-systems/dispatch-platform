import { dateFormatter } from './date-format.js';

// Platform owner pages span every DSP, so they show the viewer's device time.
export const deviceTimezone = () => Intl.DateTimeFormat().resolvedOptions().timeZone;
// DSP pages pass the DSP's timezone so every member reads the same clock.
export const time = (value: string | null | undefined, timeZone: string, empty = 'Never') =>
  value
    ? dateFormatter('en-US', {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        timeZone,
      }).format(new Date(value))
    : empty;
/** "Oct 2, 2026", in the viewer's own time, as platform pages show a day. */
export const calendarDay = (value: string) =>
  dateFormatter('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(
    new Date(value),
  );
/** "Oct 2", or "Oct 2, 2025" outside the current year: the day as UTC counts it, for
 * limits that start again at UTC midnight. */
export function utcDay(value: string, now = Date.now()) {
  const at = Date.parse(value);
  const year = (ms: number) => new Date(ms).getUTCFullYear();
  return dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    ...(year(at) === year(now) ? {} : { year: 'numeric' }),
    timeZone: 'UTC',
  }).format(at);
}
export const timeOfDay = (value: string, timeZone: string) =>
  dateFormatter('en-US', { hour: 'numeric', minute: '2-digit', timeZone }).format(new Date(value));
// To the second, for logs whose entries come many to a minute.
export const timeWithSeconds = (value: string, timeZone: string) =>
  dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
    timeZone,
  }).format(new Date(value));
// "Pacific Time" for America/Los_Angeles; the zone ID when the runtime has no name for it.
export const timezoneName = (timeZone: string) => {
  try {
    return (
      dateFormatter('en-US', { timeZone, timeZoneName: 'longGeneric' })
        .formatToParts(new Date())
        .find((part) => part.type === 'timeZoneName')?.value ?? timeZone
    );
  } catch {
    return timeZone;
  }
};
export const title = (value: string) =>
  value
    .replaceAll('_', ' ')
    .replaceAll('.', ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
export function duration(ms: number | null) {
  if (ms === null) return '—';
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`;
}
// Whole seconds, as the audit log records them.
export function elapsed(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  return minutes ? `${minutes}m ${seconds % 60}s` : `${seconds}s`;
}
/** The time left until `until`, as a countdown reads it: "9:42", and "0:00" once it has passed. */
export function countdown(until: string, now = Date.now()) {
  const seconds = Math.max(0, Math.ceil((Date.parse(until) - now) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
export const bytes = (value: number, unit: 'MiB' | 'GiB', digits = 0) =>
  `${(value / 1024 ** (unit === 'GiB' ? 3 : 2)).toFixed(digits)} ${unit}`;
