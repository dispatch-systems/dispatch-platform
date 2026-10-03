import { z } from 'zod';
import type { AuditPage, PlatformHealth } from './platform-owner.js';

const text = z.string();
const count = z.number().int().nonnegative();
export const platformHealthSchema = z.object({
  environment: z.enum(['preview', 'production']),
  release: text,
  jobs: z.partialRecord(
    z.enum(['queued', 'running', 'waiting_verification', 'succeeded', 'failed', 'cancelled']),
    count,
  ),
  browsers: z.object({
    active: count,
    capacity: count,
    memory: z.object({
      availableBytes: count.nullable(),
      requiredBytes: count,
      canStart: z.boolean(),
    }),
  }),
  dsps: count,
  email: z.boolean(),
  mail: z.object({
    enabled: z.boolean(),
    pending: count,
    failed: count,
    oldestPendingAgeMs: count.nullable(),
    lastSuccessAt: text.nullable(),
    lastAttemptAt: text.nullable(),
    lastError: text.nullable(),
    transport: z.object({ error: text.nullable(), checkedAt: text.nullable() }),
  }),
  providerMode: z.enum(['fixture', 'native']),
}) satisfies z.ZodType<PlatformHealth>;
const area = z.enum([
  'team',
  'roles',
  'collections',
  'schedules',
  'connections',
  'access',
  'dsps',
  'settings',
]);
const named = z.object({ id: text, name: text });
export const auditPageSchema = z.object({
  events: z.array(
    z.object({
      id: count,
      at: text,
      actorId: text.nullable(),
      actorName: text,
      dspId: text.nullable(),
      dspName: text.nullable(),
      action: text,
      detail: text,
      area,
      target: text.nullable(),
      ref: z.object({ kind: z.enum(['member', 'role', 'schedule', 'job']), id: text }).nullable(),
      changes: z.array(z.object({ field: text, from: text.nullable(), to: text.nullable() })),
    }),
  ),
  total: count,
  counts: z.partialRecord(z.union([area, z.literal('failures')]), count),
  actors: z.array(named),
  dsps: z.array(named),
}) satisfies z.ZodType<AuditPage>;
