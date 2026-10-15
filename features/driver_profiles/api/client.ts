import type { Job, ScheduleInput, SchedulePreview } from '../../../core/collection/api/index.js';
import { useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { RosterStatus } from './generated/RosterStatus.js';

// Driver Profiles' endpoints, as Driver Match's tab calls them.
const api = wordedApi({});

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
export type { RosterStatus };
