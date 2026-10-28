import { api, useCachedData, useData } from '../../shell/frontend/runtime/api.js';
import type {
  CollectionUpdates,
  Collectors,
  Connection,
  Job,
  LiveReading,
  ScheduleCollection,
  ScheduledCollection,
  ScheduleRuleInput,
  ScheduleRuns,
} from './index.js';
import { collectorsRead } from './urls.js';

// Collection's endpoints, as the frontend calls them: live updates, the Collectors page's
// schedules, how a live reader reads, and connections.

export const getCollectionUpdates = (after: string, signal: AbortSignal) =>
  api<CollectionUpdates>(
    `/api/dsp/collection-updates?after=${encodeURIComponent(after)}`,
    undefined,
    signal,
  );

/** The collections whose schedules the member sets, read again every `poll` milliseconds when given. */
export const useCollectors = (dspId: string, poll = 0) =>
  useCachedData<Collectors>(collectorsRead, poll, dspId);
const collection = (name: ScheduleCollection) => `/api/dsp/collectors/${encodeURIComponent(name)}`;
/** Saves a collection's rules together, as at the revision they were read at. */
export const saveRules = (name: ScheduleCollection, rules: ScheduleRuleInput[], revision: string) =>
  api<ScheduledCollection>(`${collection(name)}/schedule`, { rules, revision });
/** Turns a collection's schedule on or off, every rule at once. */
export const switchSchedule = (name: ScheduleCollection, enabled: boolean, revision: string) =>
  api<ScheduledCollection>(`${collection(name)}/enabled`, { enabled, revision });
/** When unsaved rules would run next, as though the collection were on. */
export const previewRules = (
  name: ScheduleCollection,
  rules: ScheduleRuleInput[],
  signal: AbortSignal,
) => api<ScheduleRuns>(`${collection(name)}/preview`, { rules }, signal);
/** Queues now what one run of a schedule reading by `collects` queues, once for `requestId`. */
export const collectNow = (
  name: ScheduleCollection,
  collects: ScheduleCollection,
  requestId: string,
) => api<Job[]>(`${collection(name)}/collect`, { collects, requestId });

/**
 * How the live reader `reader` reads for the DSP, none while it reads nothing there: read again
 * every `poll` milliseconds, and as `refreshKey` changes.
 */
export const useLiveReading = (reader: string, poll: number, refreshKey: string) =>
  useData<LiveReading | null>(
    `/api/dsp/live-readers/${encodeURIComponent(reader)}`,
    poll,
    refreshKey,
  );

export const connectionUrl = (provider: Connection['provider']) =>
  `/api/dsp/connections/${provider}`;
/** A connection's state, at the address its collector reads it from. */
export const useConnection = (read: string, poll = 0) => useCachedData<Connection>(read, poll);
