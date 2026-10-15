import { z } from 'zod/mini';
import type { DriverCounts } from './index.js';
import { count, type Replies } from '../../../core/foundation/api/runtime.js';

const driverCountsSchema = z.object({
  people: count,
  review: count,
  unconfirmed: count,
  missing: count,
  drivers: count,
  helpers: count,
  office: count,
  former: count,
}) satisfies z.ZodMiniType<DriverCounts>;

/** The reply of the badge counts. */
export const replies: Replies = (route, method) =>
  method === 'GET' && route === '/api/dsp/driver-match/counts' ? driverCountsSchema : undefined;
