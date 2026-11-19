import assert from 'node:assert/strict';
import type { fixture } from '../../../../../../core/shell/tests/support/support.js';

// Northline with HR Line on: its custom ID given, the feature switched on, and a phone paired to
// each of HR Line and Dispatch Phone, which a test texts as a number would.

type Fixture = Awaited<ReturnType<typeof fixture>>;
type Client = Awaited<ReturnType<Fixture['client']>>;

export const hrLine = '/api/dsp/cjf87gqd-hr-line';
export const dispatchPhone = '/api/dsp/dispatch-phone';

/** A phone of the line at `base`, paired at Northline's address: texted as `number` texts it, at
 * `at`, else now. */
export type Phone = {
  id: string;
  key: Record<string, string>;
  texted: (number: string, body: string, at?: number) => Promise<void>;
};

/** Gives Northline HR Line's custom ID and switches HR Line on, as the owner, who then opens it. */
export async function withHrLine(owner: Client) {
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  const given = await owner.post(`/api/platform/dsps/${north.id}/custom-id`, {
    customId: 'cjf87gqd',
  });
  assert.equal(given.status, 200, given.body);
  const on = await owner.post(`/api/platform/dsps/${north.id}/features`, {
    feature: 'cjf87gqd_hr_line',
    enabled: true,
  });
  assert.equal(on.status, 200, on.body);
  await owner.select(north.id);
  return north as { id: string };
}

/** Pairs a phone numbered `number` to the line at `base`, as the owner. */
export async function paired(f: Fixture, owner: Client, base: string, number: string) {
  const made = await owner.post(`${base}/phones/pairing`);
  assert.equal(made.status, 200, made.body);
  const at = f.at('nll');
  const phone = { code: made.value.code, number, model: 'Pixel 8a', version: '1.0.0' };
  const pair = await f.request('/api/dispatch-phone/phone/pair', phone, at);
  assert.equal(pair.status, 200, pair.body);
  const key = { ...at, authorization: `Bearer ${pair.value.key}` };
  let sent = 0;
  return {
    id: pair.value.phone.id as string,
    key,
    texted: async (from: string, body: string, at = Date.now()) => {
      const text = {
        clientId: `sms-${++sent}`,
        direction: 'in',
        number: from,
        body,
        at,
      };
      const reported = await f.request('/api/dispatch-phone/phone/texts', { texts: [text] }, key);
      assert.equal(reported.status, 200, reported.body);
    },
  } satisfies Phone;
}
