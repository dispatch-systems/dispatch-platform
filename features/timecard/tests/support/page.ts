import type { Locator, Page } from '@playwright/test';
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

const shown = (day: string) => `${day.slice(5, 7)}/${day.slice(8)}/${day.slice(0, 4)}`;
/** Type a date draft into the Timecard date and finish editing it. */
export async function setDate(page: Page, day: string) {
  // By role: the open calendar's own label also contains the field's.
  const field = page.getByRole('textbox', { name: 'Paycom date' });
  // A lazy tab retains its previous date field while that content is inert.
  await expect.poll(() => field.evaluate((element) => !element.closest('[inert]'))).toBe(true);
  await field.fill(day);
  await expect(field).toHaveValue(day);
  await field.press('Enter');
  // The calendar finishes editing accepted and rejected drafts; callers check the resulting day.
  await expect(field).not.toHaveAttribute('data-typing');
}
/** The Timecard date, within `scope` when a page shows more than one, reads as this day. */
export async function expectDate(scope: Page | Locator, day: string) {
  await expect(scope.getByRole('textbox', { name: 'Paycom date' })).toHaveValue(shown(day));
}
