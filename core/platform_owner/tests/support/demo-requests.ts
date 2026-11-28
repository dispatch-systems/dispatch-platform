import { createHmac } from 'node:crypto';
import type { fixture } from '../../../shell/tests/support/support.js';

type Fixture = Awaited<ReturnType<typeof fixture>>;

/** The secret a fixture server checks the site's requests against (`FIXTURE_SECRET`). */
const SECRET = 'fixture-demo-requests-secret-0123456789';

/** What a visitor fills in on the site's form, under the site's own ID for it, `id` at its end. */
export const asked = (id: string, fields: Record<string, string> = {}) => ({
  id: `0f8c6a52-submission-${id}`,
  name: 'Maria Alvarez',
  email: 'maria@northline.example',
  dsp: 'Northline Logistics',
  message: 'We run 42 routes and want to see Meal Breaks.',
  ...fields,
});

/**
 * Sends what a visitor asked as the site would: signed with `secret`, `at` seconds since 1970,
 * to the admin's address unless `headers` name another (`f.at`).
 */
export const fromSite = (
  f: Fixture,
  sent: unknown,
  {
    at = Math.floor(Date.now() / 1000),
    secret = SECRET,
    headers = {},
  }: { at?: number; secret?: string; headers?: Record<string, string> } = {},
) =>
  f.request('/demo-requests', sent, {
    'x-dispatch-timestamp': String(at),
    'x-dispatch-signature': `v1=${createHmac('sha256', secret)
      .update(`v1:${at}:${JSON.stringify(sent)}`)
      .digest('hex')}`,
    ...headers,
  });
