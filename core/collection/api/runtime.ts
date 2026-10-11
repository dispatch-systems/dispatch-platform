import { z } from 'zod/mini';
import type { CollectionUpdates, Job, JobMetrics } from './index.js';
import { collections } from './generated/collections.js';
import {
  count,
  environment,
  milliseconds,
  text,
  type Replies,
} from '../../foundation/api/runtime.js';

const pageRead = z.object({
  ordinal: count,
  attempt: count,
  stage: z.enum(['navigation', 'content', 'extraction']),
  elapsedMs: milliseconds,
  navigationMs: milliseconds,
  contentMs: milliseconds,
  extractionMs: milliseconds,
  error: z.nullable(text),
  pendingRequests: z._default(z.nullable(count), null),
  documentState: z._default(z.nullable(z.enum(['loading', 'interactive', 'complete'])), null),
});
const metricsSchema = z.object({
  attempt: count,
  startedAt: text,
  finishedAt: z.nullable(text),
  outcome: z.enum(['running', 'succeeded', 'failed', 'cancelled', 'interrupted']),
  error: z.nullable(text),
  phase: z.nullable(
    z.enum(['starting', 'authentication', 'verification', 'collection', 'publication']),
  ),
  detail: z._default(z.nullable(text), null),
  queueMs: milliseconds,
  elapsedMs: milliseconds,
  authenticationMs: z.nullable(milliseconds),
  verificationMs: z.nullable(milliseconds),
  collectionMs: z.nullable(milliseconds),
  publicationMs: z.nullable(milliseconds),
  employees: z.nullable(count),
  timecards: z.nullable(count),
  itineraries: z._default(z.nullable(count), null),
  meals: z._default(z.nullable(count), null),
  rows: z._default(z.nullable(count), null),
  peakRssBytes: z.nullable(count),
  peakPssBytes: z.nullable(count),
  peakPrivateBytes: z.nullable(count),
  memorySamples: count,
  incompleteMemorySamples: count,
  pageReads: z.optional(
    z.object({
      completed: count,
      retries: count,
      recovered: count,
      resumed: z._default(count, 0),
      earlyReady: z._default(count, 0),
      direct: z._default(count, 0),
      spotChecked: z._default(count, 0),
      totalMs: milliseconds,
      active: z.array(pageRead),
      slowest: z.array(pageRead),
      failures: z.array(pageRead),
    }),
  ),
}) satisfies z.ZodMiniType<JobMetrics>;
const jobStatusSchema = z.enum([
  'queued',
  'running',
  'waiting_verification',
  'succeeded',
  'failed',
  'cancelled',
]);
export const jobSchema = z.object({
  id: text.check(z.minLength(1)),
  dspId: text.check(z.minLength(1)),
  dspName: text,
  environment,
  // Every registered collection's, as the collectors' manifests name them.
  kind: z.enum(collections.map((collection) => collection.kind)),
  status: jobStatusSchema,
  progress: count.check(z.lte(100)),
  message: text,
  attempt: count,
  maxAttempts: count,
  availableAt: text,
  createdAt: text,
  startedAt: z.nullable(text),
  completedAt: z.nullable(text),
  error: z.nullable(text),
  release: text,
  actorId: z.nullable(text),
  metrics: z.array(metricsSchema),
}) satisfies z.ZodMiniType<Job>;

const collectionUpdatesSchema = z.object({
  revision: text,
  changes: z.array(
    z.object({
      provider: text,
      dates: z.array(text),
      employeeCode: z.nullable(text),
      roster: z.boolean(),
    }),
  ),
}) satisfies z.ZodMiniType<CollectionUpdates>;
/** A list of jobs, as the DSP's and the platform owner's job lists answer. */
export const jobsSchema = z.array(jobSchema);

/** The replies of live collection updates and of the platform owner's job list. */
export const replies: Replies = (route, method) => {
  if (method !== 'GET') return undefined;
  if (route === '/api/dsp/collection-updates') return collectionUpdatesSchema;
  if (route === '/api/platform/jobs') return jobsSchema;
  return undefined;
};
