import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import type { fixture } from '../../../../core/shell/tests/support/support.js';

type Fixture = Awaited<ReturnType<typeof fixture>>;
type Client = Awaited<ReturnType<Fixture['client']>>;

// Fixture mode stands in for Slack: installing sends the browser straight back with a code
// naming the workspace, `fixture:<team>`, and it signs with a secret tests sign with too.
const SIGNING_SECRET = 'fixture-signing-secret';
/** What Slack carries back to Dispatch from an install's address. */
export const install = (url: string) => {
  const sent = new URL(url).searchParams;
  return { state: sent.get('state')!, code: sent.get('code')! };
};
/** Adds Dispatch to fixture workspace `team`, or the next free one, for the DSP open. */
export async function connect(client: Client, team?: string) {
  const started = await client.post('/api/dsp/slack/connect');
  assert.equal(started.status, 200, started.body);
  const { state, code } = install(started.value.url);
  return client.post('/api/dsp/slack/connect/finish', {
    state,
    code: team ? `fixture:${team}` : code,
  });
}
/** Sends `body` to Slack's address `path` as Slack would, signed `at` seconds since 1970. */
export const fromSlack = (
  f: Fixture,
  path: string,
  body: string,
  { at = Math.floor(Date.now() / 1000), secret = SIGNING_SECRET } = {},
) =>
  fetch(`${f.env.DISPATCH_ORIGIN}${path}`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-slack-request-timestamp': String(at),
      'x-slack-signature': `v0=${createHmac('sha256', secret).update(`v0:${at}:${body}`).digest('hex')}`,
    },
    body,
  });
/** An event Slack sends of workspace `team`. */
export const event = (team: string, kind: Record<string, unknown>) =>
  JSON.stringify({
    type: 'event_callback',
    team_id: team,
    event_id: `Ev${Math.random().toString(36).slice(2)}`,
    event: kind,
  });
