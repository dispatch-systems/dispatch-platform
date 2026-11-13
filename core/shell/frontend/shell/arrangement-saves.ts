// A member's arrangements of the sidebar on their way to the server: one save at a time, the
// latest last, and none ahead of it once a later one waits. They outlast the list they were
// arranged in, so one arranged just before Done is still kept.
type Save = { send: () => Promise<unknown>; failed: (error: unknown) => void };
let waiting: Save | undefined;
let busy = false;

/** Sends a save in its turn, telling `failed` why if it can't be kept. */
export async function keepArrangement(send: Save['send'], failed: Save['failed']) {
  waiting = { send, failed };
  if (busy) return;
  busy = true;
  while (waiting) {
    const next = waiting;
    waiting = undefined;
    await next.send().catch(next.failed);
  }
  busy = false;
}
