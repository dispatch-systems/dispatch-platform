import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import type { RouteRetention } from './index.js';

// Routes' endpoints, as its settings panel calls them.

const routeRetention = '/api/dsp/routes/retention';
/** How long the DSP keeps its route data, and what it holds now. */
export const useRouteRetention = () => useCachedData<RouteRetention>(routeRetention);
/** Keeps `days` of route data, or every day with `null`. */
export const setRouteRetention = (days: number | null) =>
  api<RouteRetention>(routeRetention, { days });
