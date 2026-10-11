import { checkReply, type Method } from '../../../foundation/api/runtime.js';
import { replies as accounts } from '../../../accounts/api/runtime.js';
import { replies as collection } from '../../../collection/api/runtime.js';
import { replies as platformOwner } from '../../../platform_owner/api/runtime.js';
import { loadReplyChecks } from './slots.js';

const core = [accounts, collection, platformOwner];

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
  const owners = core.some((replies) => replies(route, method)) ? core : await loadReplyChecks();
  return checkReply(owners, path, method, value);
}
