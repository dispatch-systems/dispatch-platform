import { test as base, expect, type Page } from '@playwright/test';
import { built, demo, fixture, type FixtureOptions } from './support.js';

export type Dispatch = Awaited<ReturnType<typeof fixture>>;
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
