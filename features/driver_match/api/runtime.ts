import { z } from 'zod/mini';
import type { MatchCounts } from './index.js';
import { count, type Replies } from '../../../core/foundation/api/runtime.js';

const matchCountsSchema = z.object({ questions: count }) satisfies z.ZodMiniType<MatchCounts>;

/** The reply of the badge counts. */
export const replies: Replies = (route, method) =>
  method === 'GET' && route === '/api/dsp/driver-match/counts' ? matchCountsSchema : undefined;
