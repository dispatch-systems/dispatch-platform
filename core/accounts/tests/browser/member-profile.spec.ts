import { test, expect } from '../../../shell/tests/support/fixtures.js';
import { fits } from '../support/member-profile.js';

test('an expired invitation has its own page that returns to the separate sign-in page', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const assets: string[] = [];
  page.on('request', (request) => assets.push(request.url()));
  await page.goto('/#invite?token=expired-invitation');
  await expect(page.getByRole('heading', { name: 'Invitation expired' })).toBeVisible();
  await expect(
    page.getByText('This invitation has expired or was revoked. Ask your DSP for a new one.'),
  ).toBeVisible();
  await expect(page.getByLabel('First name', { exact: true })).toHaveCount(0);
  await fits(page);
  expect(assets.filter((url) => /(?:onboarding|member-profile)-map.*\.svg/.test(url))).toEqual([]);
  await page.setViewportSize({ width: 1280, height: 800 });
  await expect(page.locator('.member-profile-map')).toHaveAttribute('data-route', 'cancelled');
  await fits(page);
  await page.getByRole('button', { name: 'Go to sign in', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Sign in', exact: true })).toBeVisible();
  await expect(page.locator('.member-profile-page, .onboarding-page')).toHaveCount(0);
  await expect(page.locator('.auth-layout')).toBeVisible();
});
