import type { Job, ScheduleInput, SchedulePreview } from '../../../core/collection/api/index.js';
import { useCachedData, useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DriverDetails, DriverMatch, DriverSource, RosterStatus } from './index.js';

// Driver Profiles' endpoints, and Driver Match's, as Driver Match's tab calls them.

const driverMatch = '/api/dsp/driver-match';
// Only its own screens raise these, so their words load with them.
const api = wordedApi({
  driver_changed: 'This driver changed in another session. Refresh Driver Match and try again.',
  driver_single_id: 'This driver has only one ID, so there is nothing to split off.',
  driver_code_unavailable: 'Dispatch could not give the new driver a code. Try again.',
});
/** Everyone with a Driver Match code, and the pairs that may be one person. */
export const useDriverMatch = () => useCachedData<DriverMatch>(driverMatch);
export const useDriverDetails = (code: string) =>
  useCachedData<DriverDetails>(`${driverMatch}/drivers/${encodeURIComponent(code)}`);
/** `code`'s IDs move to `into`, whose code both keep from then on. */
export const mergeDrivers = (code: string, into: string) =>
  api<DriverMatch>(`${driverMatch}/merge`, { code, into });
/** One of `code`'s IDs moves to a new person. */
export const splitDriver = (code: string, source: DriverSource, id: string) =>
  api<DriverMatch>(`${driverMatch}/split`, { code, source, id });
export const keepDriversApart = (code: string, other: string) =>
  api<DriverMatch>(`${driverMatch}/apart`, { code, other });

/** When the roster was collected, what it holds, and when it is collected next. */
export const rosterStatusUrl = '/api/dsp/driver-profiles/status';
export const useRosterStatus = () => useData<RosterStatus>(rosterStatusUrl);
/** Starts collecting the roster, once for `requestId`. */
export const collectRoster = (requestId: string) =>
  api<Job>('/api/dsp/driver-profiles/collect', { requestId });
export const cancelRosterJob = (id: string) =>
  api(`/api/dsp/driver-profiles/jobs/${id}/cancel`, {});
/** Where its schedules are served, as core's schedule calls take it. */
export const rosterSchedules = '/api/dsp/driver-profiles/schedules';
/** When a schedule with this timing would run next. */
export const previewRosterSchedule = (
  timing: Pick<ScheduleInput, 'cadence' | 'intervalMinutes' | 'localTime'> & {
    scheduleId?: string;
  },
  signal: AbortSignal,
) => api<SchedulePreview>(`${rosterSchedules}/preview`, timing, signal);
