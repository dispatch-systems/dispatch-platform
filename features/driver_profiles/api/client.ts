import type { Job } from '../../../core/collection/api/index.js';
import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import type { RosterStatus } from './index.js';

// The roster's endpoints, as its bar on Driver Match's tab calls them.

/** When the roster was collected, what it holds, and when it is collected next. Cached, so a
 * finished collection announced to the page refreshes it, wherever it was started. */
export const useRosterStatus = (poll = 0) =>
  useCachedData<RosterStatus>('/api/dsp/driver-profiles/status', poll);
/** Starts collecting the roster, once for `requestId`. */
export const collectRoster = (requestId: string) =>
  api<Job>('/api/dsp/driver-profiles/collect', { requestId });
/** Where the roster's schedules are served, as core's schedule calls take it. */
export const rosterSchedules = '/api/dsp/driver-profiles/schedules';
