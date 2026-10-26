import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, its demo's stand-in phone paired, and its owner
// holds every permission.

test('Add phone shows the code that pairs the next phone, and a rename asks before it closes', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Connections', exact: true })
    .click();
  const card = page
    .getByRole('article')
    .filter({ has: page.getByRole('heading', { name: 'Phones', level: 3 }) });
  await expect(card.getByRole('list', { name: 'Phones' })).toContainText('(206) 555-0100');
  await card.getByRole('button', { name: 'Add phone' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add phone' });
  await expect(dialog.getByRole('img', { name: /^Pairing code [A-HJ-NP-Z2-9]{8}$/ })).toBeVisible();
  await expect(dialog).toContainText(/Expires in \d+:\d\d/);
  await page.keyboard.press('Escape');

  // A name changed and not saved is never just closed.
  await card.getByLabel('Actions for Dispatch').click();
  await page.getByRole('button', { name: 'Rename', exact: true }).click();
  const rename = page.getByRole('dialog', { name: 'Rename phone' });
  await rename.getByLabel('Name').fill('Station 2');
  await page.keyboard.press('Escape');
  const asking = page.getByRole('dialog', { name: 'Save changes?' });
  await expect(asking.getByRole('button')).toHaveText(['Discard changes', 'Save changes']);
});
