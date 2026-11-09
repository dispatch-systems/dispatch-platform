import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission.

test('a driver is written up from the page, then marked signed and locked', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Conduct', exact: true }).click();
  await page.getByRole('button', { name: 'New writeup' }).click();

  const driver = page.getByLabel('Driver');
  const name = await driver.locator('option').nth(1).textContent();
  await driver.selectOption({ index: 1 });
  await page.getByLabel('Category').selectOption({ label: 'Uniform' });
  // The category's wording fills the form, its blanks still to fill.
  const statement = page.getByLabel('1. Statement of the problem');
  await expect(statement).toHaveValue(/\[missing item\]/);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Fill in the blanks marked in yellow.');
  await statement.fill(
    'DA did not arrive to work in full uniform. DA did not have on a safety vest.',
  );
  await page.getByRole('button', { name: 'Save', exact: true }).click();

  await expect(page.getByRole('heading', { level: 1, name: name! })).toBeVisible();
  await expect(page.getByText('Awaiting signature')).toBeVisible();
  await page.getByRole('button', { name: 'Mark signed' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Mark signed' }).click();
  await expect(page.getByText(/^Signed \d+\/\d+\/\d{4}$/)).toBeVisible();
  // Signed, it can only be printed.
  await expect(page.getByRole('button', { name: 'Edit' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Print' })).toBeVisible();
});
