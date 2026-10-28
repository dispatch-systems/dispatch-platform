import type { Conversation } from '../../api/index.js';
import { openConversation, textDriver, textFirst } from '../../api/client.js';

/**
 * A conversation to open, each with what opening it needs: one that exists, by its ID; or one
 * started, with a member of a workspace, a number, or one of today's drivers, at their work phone.
 */
export type Target =
  | { kind: 'conversation'; id: string }
  | { kind: 'slack'; teamId: string; userId: string }
  | { kind: 'number'; number: string }
  | { kind: 'driver'; driverId: string };

/** Starts the conversation `target` names: the one there is, as the server opens it. */
const starts: {
  [K in Exclude<Target['kind'], 'conversation'>]: (
    target: Extract<Target, { kind: K }>,
  ) => Promise<Conversation>;
} = {
  slack: ({ teamId, userId }) => openConversation(teamId, userId),
  number: ({ number }) => textFirst(number),
  driver: ({ driverId }) => textDriver(driverId),
};
/** The conversation a target that starts one opens. */
export const started = (target: Exclude<Target, { kind: 'conversation' }>) =>
  (starts[target.kind] as (target: Target) => Promise<Conversation>)(target);

/** A conversation known by its ID, else `start`ed. */
export const existingOr = (id: string | null, start: Target): Target =>
  id ? { kind: 'conversation', id } : start;
