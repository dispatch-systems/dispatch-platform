import { checkReply, type Method } from '../../../foundation/api/runtime.js';
import { replies as accounts } from '../../../accounts/api/runtime.js';
import { replies as collection } from '../../../collection/api/runtime.js';
import { loadReplyChecks } from './slots.js';

const core = [accounts, collection];

/**
 * A reply, checked by core's validators for its route or else by those an installed owner gives
 * in its manifest. Core's run without waiting; a reply they don't check waits for the owners' to
 * load. No two owners check the same route.
 */
export async function parseApiResponse(
  path: string,
  method: Method,
  value: unknown,
): Promise<unknown> {
  const route = path.split('?')[0]!;
  if (core.some((replies) => replies(route, method))) return checkReply(core, path, method, value);
  if (route.startsWith('/api/platform/')) {
    // Its audit subjects include every owner's declarations. Only a successful platform
    // response needs this validator; the asset server admits its chunk to platform owners.
    const { replies } = await import('../../../platform_owner/api/runtime.js');
    if (replies(route, method)) return checkReply([replies], path, method, value);
  }
  return checkReply(await loadReplyChecks(), path, method, value);
}
