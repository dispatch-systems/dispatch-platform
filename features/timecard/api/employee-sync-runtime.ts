import { z } from 'zod/mini';
import { count, text } from '../../../core/foundation/api/runtime.js';
import { jobsSchema } from '../../../core/collection/api/runtime.js';
import type { EmployeeSyncStatus, EmployeeSyncSource } from './index.js';

const source = z.object({
  enabled: z.boolean(),
  active: z.boolean(),
  total: count,
  completed: count,
  failed: count,
  cancelled: count,
  progress: count,
  jobs: jobsSchema,
}) satisfies z.ZodMiniType<EmployeeSyncSource>;

export const employeeSyncSchema = z.object({
  period: z.object({ from: text, to: text }),
  through: text,
  requestId: z.nullable(text),
  scopeAvailable: z.boolean(),
  active: z.boolean(),
  complete: z.boolean(),
  paycom: source,
  cortex: source,
}) satisfies z.ZodMiniType<EmployeeSyncStatus>;
