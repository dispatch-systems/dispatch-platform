import { checkReply, type Method } from '../../../../shared/contracts/runtime.js';
import { replies as accounts } from '../../../../shared/contracts/runtime-accounts.js';
import { replies as collection } from '../../../../shared/contracts/runtime-collection.js';
import { replies as platformOwner } from '../../../../shared/contracts/runtime-platform-owner.js';
import { replyChecks } from './slots.js';

/**
 * A reply, checked by core's validators for its route or else by those an installed owner gives
 * in its manifest. No two owners check the same route.
 */
export const parseApiResponse = (path: string, method: Method, value: unknown): unknown =>
  checkReply([accounts, collection, platformOwner, ...replyChecks()], path, method, value);
