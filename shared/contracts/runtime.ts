import { z } from 'zod';

// The validators every owner's API runtime builds on, and how the frontend checks a reply with
// the validators the owners give.
export const text = z.string();
export const count = z.number().int().nonnegative();
export const milliseconds = z.number().nonnegative();
export const environment = z.enum(['preview', 'production']);
export const providerMode = z.enum(['fixture', 'native']);

export type Method = 'GET' | 'POST';
/** An owner's reply validators: the one for a route's reply to a method, if it checks that. */
export type Replies = (route: string, method: Method) => z.ZodType | undefined;

/**
 * A reply, checked by the first of `owners` that has a validator for its route. A reply none
 * of them checks passes as it came.
 */
export function checkReply(
  owners: readonly Replies[],
  path: string,
  method: Method,
  value: unknown,
): unknown {
  const route = path.split('?')[0]!;
  let schema: z.ZodType | undefined;
  for (const replies of owners) if ((schema = replies(route, method))) break;
  if (!schema) return value;
  const result = schema.safeParse(value);
  // Never expose a server payload, session token or employee data in an error.
  if (!result.success) throw new Error('invalid_api_response');
  return result.data;
}
