import type { Page } from '@playwright/test';
import type { SessionView } from '../../../../core/accounts/api/index.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { demo, expect, type Dispatch } from '../../../../core/shell/tests/support/fixtures.js';

/** Opens Timecard's page: opt-in setup for its tests rather than the sign-in form or DSP picker.
 * The real login endpoint sets the context's HttpOnly cookie; the dashboard still
 * bootstraps its session and obtains its own admitted DSP view through the real API.
 */
export async function openAuthenticatedDsp(page: Page, dispatch: Dispatch, name: string) {
  const origin = dispatch.env.DISPATCH_ORIGIN;
  const signedIn = await page.request.post(`${origin}/api/auth/login`, {
    headers: { origin },
    data: { email: demo.email, password: demo.password },
  });
  expect(signedIn.status()).toBe(200);
  const response = await page.request.get(`${origin}/api/session`);
  expect(response.status()).toBe(200);
  const session = (await response.json()) as SessionView;
  const dsp = session.dsps.find((item) => item.name === name);
  expect(dsp, `seeded DSP ${name}`).toBeDefined();
  await page.goto(`/${dspHash(dsp!.id, 'paycom')}`);
}
