import { test, expect, login } from '../../../shell/tests/support/fixtures.js';
import { capturedMail } from '../../../shell/tests/support/mail-support.js';

async function ownerInvitation(page: import('@playwright/test').Page, root: string) {
  await login(page);
  await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
  const session = await (await page.request.get('/api/session')).json();
  const origin = new URL(page.url()).origin;
  const created = await page.request.post('/api/platform/dsps', {
    headers: { Origin: origin, 'X-CSRF-Token': session.csrf },
    data: { ownerEmail: 'responsive-owner@dispatch.test' },
  });
  expect(created.status()).toBe(201);
  const mail = await capturedMail(root, 'responsive-owner@dispatch.test');
  return `${origin}/#invite?token=${/token=([A-Za-z0-9_-]{43})/.exec(mail.text)![1]}`;
}

async function fits(page: import('@playwright/test').Page) {
  await expect
    .poll(() =>
      page.evaluate(() => {
        const button = document.querySelector<HTMLButtonElement>(
          '.onboarding-panel form:not([hidden]) .primary',
        )!;
        const title = document.querySelector('h1')!.getBoundingClientRect();
        const bounds = button.getBoundingClientRect();
        return (
          document.documentElement.scrollWidth <= innerWidth &&
          document.documentElement.scrollHeight <= innerHeight &&
          title.top >= 0 &&
          bounds.bottom <= innerHeight &&
          bounds.left >= 0 &&
          bounds.right <= innerWidth
        );
      }),
    )
    .toBe(true);
}

test('desktop reveals map and form together and remains usable if the map fails', async ({
  page,
  browser,
  dispatch,
}) => {
  const url = await ownerInvitation(page, dispatch.root);
  // SVG <use> consumers may include their region fragment in the routed URL.
  const mapAsset = /\/onboarding-map-[^/]+\.svg(?:[#?].*)?$/;
  let release!: () => void;
  let waiting = false;
  const delayed = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(mapAsset, async (route) => {
    waiting = true;
    await delayed;
    await route.continue();
  });
  try {
    await page.goto(url, { waitUntil: 'domcontentloaded' });
    const heading = page.getByRole('heading', { name: 'Set up your DSP' });
    await expect.poll(() => waiting).toBe(true);
    await expect(page.locator('.onboarding-page')).toHaveAttribute('data-ready', 'false');
    await expect(heading).toBeHidden();
    await expect(page.locator('.onboarding-map')).toBeHidden();
    release();
    await expect(heading).toBeVisible();
    await expect(page.locator('.onboarding-map')).toBeVisible();
    await fits(page);
  } finally {
    release();
  }
  await page.unroute(mapAsset);
  // Invitations remove their token from the live hash. A fresh context opens the
  // original address without reusing the successful page's map or image cache.
  const failedContext = await browser.newContext({ viewport: page.viewportSize()! });
  try {
    const failedPage = await failedContext.newPage();
    let failedMaps = 0;
    await failedPage.route(mapAsset, (route) => {
      failedMaps++;
      return route.abort();
    });
    await failedPage.goto(url, { waitUntil: 'domcontentloaded' });
    await expect.poll(() => failedMaps).toBeGreaterThan(0);
    await expect(failedPage.getByRole('heading', { name: 'Set up your DSP' })).toBeVisible();
    await fits(failedPage);
    await failedPage
      .getByLabel('DSP name', { exact: true })
      .fill('Available even without the illustration');
  } finally {
    await failedContext.close();
  }
});
