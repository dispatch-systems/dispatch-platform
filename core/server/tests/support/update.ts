import type { Locator, Page, Request } from '@playwright/test';
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

/**
 * Runs the frozen test clock for `ms`, a second at a time, letting every update check the page
 * starts be answered before the clock moves on. A check gives up after five seconds of the
 * page's clock, the test clock included: one long jump would abort it before its real answer
 * arrived, and the page would never learn of the update.
 */
export async function runAnswered(page: Page, ms: number) {
  const checks = new Set<Request>();
  const started = (request: Request) => {
    if (new URL(request.url()).pathname === '/api/browser-update') checks.add(request);
  };
  const ended = (request: Request) => checks.delete(request);
  page.on('request', started);
  page.on('requestfinished', ended);
  page.on('requestfailed', ended);
  try {
    for (let ran = 0; ran < ms; ran += 1000) {
      await page.clock.runFor(Math.min(1000, ms - ran));
      await expect.poll(() => checks.size).toBe(0);
    }
  } finally {
    page.off('request', started);
    page.off('requestfinished', ended);
    page.off('requestfailed', ended);
  }
}
