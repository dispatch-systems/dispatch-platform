import assert from 'node:assert/strict';
import { type fixture, until } from '../../../shell/tests/support/support.js';

type Client = Awaited<ReturnType<Awaited<ReturnType<typeof fixture>>['client']>>;

/**
 * Collects `collection` now as `client`, as Collect now on Settings' Collectors page does, and
 * waits until its run ends, which it asserts succeeded. Answers the jobs it queued.
 */
export async function collectNow(
  client: Client,
  collection: string,
  requestId: string,
  timeout?: number,
) {
  const queued = await client.post(`/api/dsp/collectors/${collection}/collect`, {
    collects: collection,
    requestId,
  });
  assert.equal(queued.status, 202, queued.body);
  let shown: { running: boolean; lastRun: { status: string } | null } | undefined;
  await until(async () => {
    shown = (await client.read('/api/dsp/collectors')).collections.find(
      (each: { collection: string }) => each.collection === collection,
    );
    return !shown!.running;
  }, timeout);
  assert.equal(shown!.lastRun?.status, 'succeeded', JSON.stringify(shown!.lastRun));
  return queued.value;
}
