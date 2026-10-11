import { z } from 'zod/mini';
import type {
  UniformInventory,
  UniformAdjustment,
  UniformUpdates,
  UniformHistory,
} from './index.js';
import { count, type Replies } from '../../../core/foundation/api/runtime.js';

const fit = z.enum(['men', 'women', 'unisex']);
const variant = z.object({
  id: z.string(),
  fit,
  size: z.string(),
  quantity: count,
  revision: count,
});
export const uniformInventorySchema = z.object({
  revision: count,
  uniforms: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      category: z.string(),
      revision: count,
      variants: z.array(variant),
    }),
  ),
}) satisfies z.ZodMiniType<UniformInventory>;
export const uniformAdjustmentSchema = z.object({
  revision: count,
  variantId: z.string(),
  quantity: count,
}) satisfies z.ZodMiniType<UniformAdjustment>;
export const uniformUpdatesSchema = z.object({
  revision: count,
  inventory: z.nullable(uniformInventorySchema),
  adjustments: z.array(uniformAdjustmentSchema),
}) satisfies z.ZodMiniType<UniformUpdates>;
export const uniformHistorySchema = z.object({
  events: z.array(
    z.object({
      revision: count,
      kind: z.enum(['initialized', 'created', 'updated', 'archived', 'adjusted']),
      uniformName: z.string(),
      fit: z.nullable(fit),
      size: z.nullable(z.string()),
      delta: z.nullable(z.number().check(z.int())),
      quantity: z.nullable(count),
      actorName: z.string(),
      at: z.string(),
    }),
  ),
  nextBefore: z.nullable(count),
}) satisfies z.ZodMiniType<UniformHistory>;

/** The replies of the inventory, its live updates and history, and its edits. */
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/dsp/uniforms') return uniformInventorySchema;
    if (route === '/api/dsp/uniforms/updates') return uniformUpdatesSchema;
    if (route === '/api/dsp/uniforms/history') return uniformHistorySchema;
    return undefined;
  }
  if (route.startsWith('/api/dsp/uniforms/stock/')) return uniformAdjustmentSchema;
  if (route === '/api/dsp/uniforms' || route.startsWith('/api/dsp/uniforms/'))
    return uniformInventorySchema;
  return undefined;
};
