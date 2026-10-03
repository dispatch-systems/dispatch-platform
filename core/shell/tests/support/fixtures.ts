import { test as base, expect, type Locator, type Page } from '@playwright/test';
import { built, demo, fixture, type FixtureOptions } from './support.js';
import type { SessionView } from '../../../../shared/contracts/index.js';
import { dspHash } from '../../frontend/runtime/navigation.js';

type Dispatch = Awaited<ReturnType<typeof fixture>>;
export const test = base.extend<{
  /** Set with `test.use`, or override in `test.extend`, to change the server under test. */
  dispatchOptions: Pick<FixtureOptions, 'seed' | 'env'>;
  /** A private server of the built artifact with its own state, port and mail. */
  dispatch: Dispatch;
  /** Set with `test.use` to load the sign-in van's model, for the tests about it. */
  signInAnimation: boolean;
}>({
  dispatchOptions: [{}, { option: true }],
  signInAnimation: [false, { option: true }],
  // Chromium renders the van in software here, which costs seconds of every sign-in and
  // has nothing to do with what most tests assert. Its renderer is served as a module
  // that never starts, so the page keeps the poster it shows until the van is ready and
  // fetches no model, while the van's own tests get the real one.
  page: async ({ page, signInAnimation }, use) => {
    if (!signInAnimation)
      await page.route('**/renderer-*.js', (route) =>
        route.fulfill({
          status: 200,
          contentType: 'text/javascript',
          body: 'export function startVan() {}\n',
        }),
      );
    await use(page);
  },
  dispatch: async ({ dispatchOptions }, use) => {
    const app = await fixture({
      ...dispatchOptions,
      ...built,
      env: { ...built.env, ...dispatchOptions.env },
    });
    try {
      await use(app);
    } finally {
      await app.close();
    }
  },
  baseURL: async ({ dispatch }, use) => {
    await use(dispatch.env.DISPATCH_ORIGIN);
  },
});
export { demo, expect };

/** Fill and submit the sign-in form the page already shows. */
export async function signIn(page: Page, email = demo.email) {
  await page.getByLabel('Email address').fill(email);
  await page.getByLabel('Password', { exact: true }).fill(demo.password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
}
/** Open the app and sign in, as the platform owner unless another seeded account is named. */
export async function login(page: Page, email = demo.email) {
  await page.goto('/');
  await signIn(page, email);
}
/** From the platform's DSP list, choose a DSP and enter its view from its pane. */
export async function openDsp(page: Page, name: string) {
  await page
    .getByRole('region', { name: 'DSPs', exact: true })
    .getByRole('button', { name: new RegExp(name) })
    .click();
  await page
    .getByRole('region', { name: new RegExp(name) })
    .getByRole('button', { name: 'View', exact: true })
    .click();
}

/** Opt-in setup for tests of a DSP page rather than the sign-in form or DSP picker.
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
