import { useCachedData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { RouteRetention } from './index.js';

// Routes' endpoints, as its settings panel calls them. Only its own screens raise these, so
// their words load with them.
const api = wordedApi({
  invalid_retention: 'Choose a retention window from 30 to 3,650 days.',
  routes_day_outside_retention:
    'That day is older than your route data retention window. Lengthen the window first.',
});

const routeRetention = '/api/dsp/routes/retention';
/** How long the DSP keeps its route data, and what it holds now. */
export const useRouteRetention = () => useCachedData<RouteRetention>(routeRetention);
/** Keeps `days` of route data, or every day with `null`. */
export const setRouteRetention = (days: number | null) =>
  api<RouteRetention>(routeRetention, { days });
