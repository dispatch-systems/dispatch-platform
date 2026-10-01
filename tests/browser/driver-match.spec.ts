import { test, expect, login, openDsp } from './fixtures.js';
import { seedDriverMatch } from '../support/driver-match.js';

test('Driver Match shows who might be listed twice and makes them one person', async ({
  page,
  dispatch,
}) => {
  await seedDriverMatch(dispatch);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  const tab = page.getByRole('tab', { name: /Driver Match/ });
  await expect(tab).toContainText('3');
  await tab.click();
  const review = page.getByRole('region', { name: /might be listed twice/ });
  const pairs = review.locator('.driver-pair');
  await expect(pairs).toHaveCount(3);
  const ben = pairs.filter({ hasText: 'Ben Collins' });
  await expect(ben).toContainText('Ben is short for Benjamin');
  await expect(ben).toContainText('Strong match');
  await expect(pairs.filter({ hasText: 'Drew S.' })).toContainText('Possible match');
  await page.screenshot({ path: test.info().outputPath('driver-match.png'), fullPage: true });

  await ben.getByRole('button', { name: 'Same person', exact: true }).click();
  await expect(
    page.getByText('Benjamin Collins and Ben Collins are now one person', { exact: true }),
  ).toBeVisible();
  await expect(pairs).toHaveCount(2);
  await expect(tab).toContainText('2');
  // Someone marked as different is never suggested again.
  await pairs
    .filter({ hasText: 'Drew S.' })
    .getByRole('button', { name: 'Different people', exact: true })
    .click();
  await expect(pairs).toHaveCount(1);

  await page.getByLabel('Search drivers').fill('benjamin');
  const row = page.getByRole('row').filter({ hasText: 'Benjamin Collins' });
  await expect(row).toContainText('Confirmed');
  await row.getByRole('button', { name: 'Open Benjamin Collins', exact: true }).click();
  const sheet = page.getByRole('dialog');
  await expect(sheet).toContainText('Goes by Ben on Amazon');
  await expect(
    sheet.getByRole('listitem').filter({ hasText: 'confirmed as this person' }),
  ).toHaveCount(1);
  await page.screenshot({ path: test.info().outputPath('driver-match-sheet.png') });
  // A wrong link comes apart again, and the two stay apart.
  await sheet
    .locator('.driver-id')
    .filter({ hasText: 'Amazon' })
    .getByRole('button', { name: 'Split off', exact: true })
    .click();
  await page.getByRole('button', { name: 'Split off', exact: true }).last().click();
  await expect(page.getByText(/is now its own person/)).toBeVisible();
  await expect(sheet.locator('.driver-id')).toHaveCount(1);
  await expect(pairs.filter({ hasText: 'Ben Collins' })).toHaveCount(0);
});

test('a member without Manage Driver Match never sees the tab', async ({ page }) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Profile', exact: true })).toBeVisible();
  await expect(page.getByRole('tab', { name: /Driver Match/ })).toHaveCount(0);
});
