import type { Locator, Page } from '@playwright/test';
import { expect, login } from '../../../shell/tests/support/fixtures.js';

/** Waits for `locator` to show, running the frozen test clock: dynamic routes commit through
 * Suspense, so their timers must run under it. */
export async function clockVisible(page: Page, locator: Locator) {
  await expect
    .poll(async () => {
      await page.clock.runFor(500);
      return locator.isVisible();
    })
    .toBe(true);
}

/** Installs the test clock, then signs in as the platform owner. */
export async function loginWithClock(page: Page) {
  await page.clock.install();
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
}
