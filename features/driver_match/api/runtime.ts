import { z } from 'zod';
import type { DriverCounts } from './index.js';
import { count, type Replies } from '../../../core/foundation/api/runtime.js';

const driverCountsSchema = z.object({
  all: count,
  drivers: count,
  matched: count,
  review: count,
  paycomOnly: count,
  amazonOnly: count,
  office: count,
  former: count,
}) satisfies z.ZodType<DriverCounts>;

/** The reply of the badge counts. */
export const replies: Replies = (route, method) =>
  method === 'GET' && route === '/api/dsp/driver-match/counts' ? driverCountsSchema : undefined;
