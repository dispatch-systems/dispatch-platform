import { performancePolicy } from '../../shell/frontend/lib/performance-policy.js';
import { api, useCachedData } from '../../shell/frontend/runtime/api.js';
import type {
  CollectionSchedule,
  CollectionSchedules,
  CollectionUpdates,
  Connection,
  ScheduleInput,
} from '../../../shared/contracts/collection.js';

// Collection's endpoints, as the frontend calls them: live updates, schedules and connections.

export const getCollectionUpdates = (after: string, signal: AbortSignal) =>
  api<CollectionUpdates>(
    `/api/dsp/collection-updates?after=${encodeURIComponent(after)}`,
    undefined,
    signal,
  );

const schedules = '/api/dsp/schedules';
export const useSchedules = (dspId: string) =>
  useCachedData<CollectionSchedules>(schedules, performancePolicy.recoveryPollMs, dspId);
export const getSchedules = () => api<CollectionSchedules>(schedules);
/** Saving an existing schedule names the revision it was read at. */
export const saveSchedule = (
  id: string | undefined,
  schedule: ScheduleInput & { revision?: number },
) => api<CollectionSchedule>(id ? `${schedules}/${id}` : schedules, schedule);
export const setScheduleEnabled = (id: string, enabled: boolean, revision: number) =>
  api<CollectionSchedule>(`${schedules}/${id}/enabled`, { enabled, revision });
export const removeSchedule = (id: string, revision: number) =>
  api(`${schedules}/${id}/remove`, { revision });

export const connectionUrl = (provider: Connection['provider']) =>
  `/api/dsp/connections/${provider}`;
/** A connection's state, at the address its collector reads it from. */
export const useConnection = (read: string, poll = 0) => useCachedData<Connection>(read, poll);
