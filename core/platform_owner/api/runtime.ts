import { z } from 'zod/mini';
import type { AuditPage, PlatformHealth } from './index.js';
import {
  count,
  environment,
  providerMode,
  text,
  type Replies,
} from '../../foundation/api/runtime.js';
import { auditSubjects } from '../../tenancy/api/generated/audit-subjects.js';

export const platformHealthSchema = z.object({
  environment,
  release: text,
  jobs: z.partialRecord(
    z.enum(['queued', 'running', 'waiting_verification', 'succeeded', 'failed', 'cancelled']),
    count,
  ),
  browsers: z.object({
    active: count,
    capacity: count,
    memory: z.object({
      availableBytes: z.nullable(count),
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
    oldestPendingAgeMs: z.nullable(count),
    lastSuccessAt: z.nullable(text),
    lastAttemptAt: z.nullable(text),
    lastError: z.nullable(text),
    transport: z.object({ error: z.nullable(text), checkedAt: z.nullable(text) }),
  }),
  providerMode,
}) satisfies z.ZodMiniType<PlatformHealth>;
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
const auditPageSchema = z.object({
  events: z.array(
    z.object({
      id: count,
      at: text,
      actorId: z.nullable(text),
      actorName: text,
      dspId: z.nullable(text),
      dspName: z.nullable(text),
      action: text,
      detail: text,
      area,
      target: z.nullable(text),
      ref: z.nullable(z.object({ kind: z.enum(auditSubjects), id: text })),
      changes: z.array(z.object({ field: text, from: z.nullable(text), to: z.nullable(text) })),
    }),
  ),
  total: count,
  counts: z.partialRecord(z.union([area, z.literal('failures')]), count),
  actors: z.array(named),
  dsps: z.array(named),
}) satisfies z.ZodMiniType<AuditPage>;

/** The replies of the platform's health and the audit log. */
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/platform/audit') return auditPageSchema;
    if (route === '/api/platform/health') return platformHealthSchema;
    return undefined;
  }
  return route === '/api/platform/audit/export' ? auditPageSchema : undefined;
};
