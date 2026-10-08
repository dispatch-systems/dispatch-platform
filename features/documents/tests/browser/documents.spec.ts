import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Google sends the browser straight back, as Google does once someone clicks Allow, and
// keeps the account's Drive in memory.

test('an owner connects Google, starts with folders and makes a Doc that opens in Google', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await page.getByRole('button', { name: 'Connect Google' }).click();
  // The code Google sent back is used once and gone from the address.
  const ready = page.getByRole('dialog', { name: 'Documents is ready' });
  await expect(ready).toContainText('Northline Logistics Documents');
  expect(page.url()).not.toContain('googleCode');
  await ready.getByRole('checkbox', { name: 'Templates' }).check();
  await ready.getByRole('button', { name: 'Add 6 folders' }).click();
  await expect(page.getByRole('link', { name: /Templates/ })).toBeVisible();

  // A Doc made in a folder opens in its own tab, at Google, which this browser only pretends
  // to reach.
  await page
    .context()
    .route('https://docs.google.com/**', (route) => route.fulfill({ body: 'Google Docs' }));
  await page.getByRole('link', { name: /Safety & Compliance/ }).click();
  await expect(page.getByRole('navigation', { name: 'Folder' })).toContainText(
    'Safety & Compliance',
  );
  await page.getByLabel('New').click();
  await page.getByRole('button', { name: 'Google Doc' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Rescue plan');
  const opened = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(await opened).toHaveURL(/^https:\/\/docs\.google\.com\/document\/d\/[\w-]+\/edit$/);
  await expect(page.getByRole('link', { name: /Rescue plan/ })).toHaveAttribute('target', '_blank');
});
