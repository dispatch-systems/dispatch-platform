import type { Job } from '../../../core/collection/api/index.js';
import { useCachedData, useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { DriverDetails, DriverMatch, DriverSource, RosterStatus } from './index.js';

// Driver Match's endpoints, and the roster's, as Driver Match's tab calls them.

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
/** One of `code`'s IDs a match joined is theirs. */
export const confirmDriver = (code: string, source: DriverSource, id: string) =>
  api<DriverMatch>(`${driverMatch}/confirm`, { code, source, id });
export const keepDriversApart = (code: string, other: string) =>
  api<DriverMatch>(`${driverMatch}/apart`, { code, other });

/** When the roster was collected, what it holds, and when it is collected next. */
export const useRosterStatus = (poll = 0) =>
  useData<RosterStatus>('/api/dsp/driver-profiles/status', poll);
/** Starts collecting the roster, once for `requestId`. */
export const collectRoster = (requestId: string) =>
  api<Job>('/api/dsp/driver-profiles/collect', { requestId });
/** Where the roster's schedules are served, as core's schedule calls take it. */
export const rosterSchedules = '/api/dsp/driver-profiles/schedules';
