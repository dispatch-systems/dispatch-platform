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

/** The page's count of the update checks it has started and not yet heard back from. */
type Counted = { updateChecks?: number };
/**
 * Counts the page's update checks in the page itself, in the very call that starts each one and
 * its five-second timeout: a request event reaches the test later, too late under load to stop
 * the clock running past the timeout. Each page the tab loads counts them, once installed.
 */
export async function countUpdateChecks(page: Page) {
  await page.addInitScript(() => {
    const counted = window as unknown as Counted;
    counted.updateChecks = 0;
    const fetch = window.fetch.bind(window);
    window.fetch = (input, init) => {
      const url = input instanceof Request ? input.url : String(input);
      if (new URL(url, location.href).pathname !== '/api/browser-update') return fetch(input, init);
      counted.updateChecks! += 1;
      return fetch(input, init).finally(() => (counted.updateChecks! -= 1));
    };
  });
}

/** Installs the test clock and counts update checks, then signs in as the platform owner. */
export async function loginWithClock(page: Page) {
  await page.clock.install();
  await countUpdateChecks(page);
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
}

/**
 * Runs the frozen test clock for `ms`, a second at a time, letting every update check the page
 * starts be answered before the clock moves on. A check gives up after five seconds of the
 * page's clock, the test clock included: one long jump would abort it before its real answer
 * arrived, and the page would never learn of the update. The page must count its checks
 * (`countUpdateChecks`) from before it loaded.
 */
export async function runAnswered(page: Page, ms: number) {
  const counted = await page.evaluate(() => (window as unknown as Counted).updateChecks);
  if (counted === undefined) throw new Error('countUpdateChecks(page) before the page loads');
  for (let ran = 0; ran < ms; ran += 1000) {
    await page.clock.runFor(Math.min(1000, ms - ran));
    // A reload the update starts takes the count with the page it leaves: until the next
    // page answers with its own, nothing is known.
    await expect
      .poll(() =>
        page.evaluate(() => (window as unknown as Counted).updateChecks).catch(() => undefined),
      )
      .toBe(0);
  }
}
