import { z } from 'zod/mini';
import { text } from '../../../core/foundation/api/runtime.js';
import { jobSchema } from '../../../core/collection/api/runtime.js';
import type { DailySyncSource, DailySyncStatus, PaycomStatus } from './index.js';

const source = z.object({
  enabled: z.boolean(),
  active: z.boolean(),
  jobDate: z.nullable(text),
  job: z.nullable(jobSchema),
  collectedAt: z.nullable(text),
}) satisfies z.ZodMiniType<DailySyncSource>;

export const dailySyncSchema = z.object({
  date: text,
  scopeAvailable: z.boolean(),
  paycom: source,
  flex: source,
}) satisfies z.ZodMiniType<DailySyncStatus>;

export const paycomStatusSchema = z.object({
  connection: z.object({
    provider: text,
    enabled: z.boolean(),
    status: z.enum(['not_connected', 'ready', 'signing_in', 'needs_verification', 'error']),
    error: z.nullable(text),
    updatedAt: text,
    lastVerifiedAt: z.nullable(text),
    accountLabel: z.nullable(text),
    verificationSessionId: z.optional(text),
    checks: z.array(
      z.object({
        id: text,
        state: z.enum(['waiting', 'passed', 'failed']),
        error: z.optional(text),
      }),
    ),
  }),
  workforce: z.object({ collectedAt: z.nullable(text) }),
}) satisfies z.ZodMiniType<PaycomStatus>;
