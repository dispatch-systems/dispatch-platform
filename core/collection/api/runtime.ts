import { z } from 'zod';
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
  error: text.nullable(),
  pendingRequests: count.nullable().default(null),
  documentState: z.enum(['loading', 'interactive', 'complete']).nullable().default(null),
});
const metricsSchema = z.object({
  attempt: count,
  startedAt: text,
  finishedAt: text.nullable(),
  outcome: z.enum(['running', 'succeeded', 'failed', 'cancelled', 'interrupted']),
  error: text.nullable(),
  phase: z
    .enum(['starting', 'authentication', 'verification', 'collection', 'publication'])
    .nullable(),
  detail: text.nullable().default(null),
  queueMs: milliseconds,
  elapsedMs: milliseconds,
  authenticationMs: milliseconds.nullable(),
  verificationMs: milliseconds.nullable(),
  collectionMs: milliseconds.nullable(),
  publicationMs: milliseconds.nullable(),
  employees: count.nullable(),
  timecards: count.nullable(),
  itineraries: count.nullable().default(null),
  meals: count.nullable().default(null),
  rows: count.nullable().default(null),
  peakRssBytes: count.nullable(),
  peakPssBytes: count.nullable(),
  peakPrivateBytes: count.nullable(),
  memorySamples: count,
  incompleteMemorySamples: count,
  pageReads: z
    .object({
      completed: count,
      retries: count,
      recovered: count,
      resumed: count.default(0),
      earlyReady: count.default(0),
      direct: count.default(0),
      spotChecked: count.default(0),
      totalMs: milliseconds,
      active: z.array(pageRead),
      slowest: z.array(pageRead),
      failures: z.array(pageRead),
    })
    .optional(),
}) satisfies z.ZodType<JobMetrics>;
const jobStatusSchema = z.enum([
  'queued',
  'running',
  'waiting_verification',
  'succeeded',
  'failed',
  'cancelled',
]);
export const jobSchema = z.object({
  id: text.min(1),
  dspId: text.min(1),
  dspName: text,
  environment,
  // Every registered collection's, as the collectors' manifests name them.
  kind: z.enum(collections.map((collection) => collection.kind)),
  status: jobStatusSchema,
  progress: count.max(100),
  message: text,
  attempt: count,
  maxAttempts: count,
  availableAt: text,
  createdAt: text,
  startedAt: text.nullable(),
  completedAt: text.nullable(),
  error: text.nullable(),
  release: text,
  actorId: text.nullable(),
  metrics: z.array(metricsSchema),
}) satisfies z.ZodType<Job>;

const collectionUpdatesSchema = z.object({
  revision: text,
  changes: z.array(
    z.object({
      provider: text,
      dates: z.array(text),
      employeeCode: text.nullable(),
      roster: z.boolean(),
    }),
  ),
}) satisfies z.ZodType<CollectionUpdates>;
/** A list of jobs, as the DSP's and the platform owner's job lists answer. */
export const jobsSchema = z.array(jobSchema);

/** The replies of live collection updates and of the job lists and the job a collect starts. */
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/dsp/collection-updates') return collectionUpdatesSchema;
    if (route === '/api/platform/jobs' || route === '/api/dsp/jobs') return jobsSchema;
    return undefined;
  }
  return route === '/api/dsp/jobs' ? jobSchema : undefined;
};
