import type { Job } from '../../../core/collection/api/index.js';
import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import type { FleetStatus } from './index.js';

// The fleet's endpoints, as its bar on its Settings page calls them.

/** When the fleet was collected, what it holds, and when it is collected next. Cached, so a
 * finished collection announced to the page refreshes it, wherever it was started. */
export const useFleetStatus = (poll = 0) =>
  useCachedData<FleetStatus>('/api/dsp/vans/status', poll);
/** Starts collecting the fleet, once for `requestId`. */
export const collectFleet = (requestId: string) => api<Job>('/api/dsp/vans/collect', { requestId });
