import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Google sends the browser straight back, as Google does once someone clicks Allow.

test('an owner connects Google and comes back to the folder Dispatch made', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await page.getByRole('button', { name: 'Connect Google' }).click();
  await expect(page.getByRole('heading', { name: 'Northline Logistics Documents' })).toBeVisible();
  // The code Google sent back is used once and gone from the address.
  expect(page.url()).not.toContain('googleCode');
});
