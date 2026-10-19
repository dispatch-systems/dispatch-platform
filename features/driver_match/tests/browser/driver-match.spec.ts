import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { seedDriverMatch } from '../support/driver-match.js';

test('Driver Match asks who might be listed twice and makes them one person', async ({
  page,
  dispatch,
}) => {
  const dsp = await seedDriverMatch(dispatch);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // Its page in the sidebar counts three pairs and a second surname to confirm.
  const link = page.getByRole('link', { name: /^Driver Match/ });
  await expect(link).toContainText('4');
  // A link to where it was, a tab of Settings, opens it.
  await page.goto(`/#dsp/${dsp.id}/settings?tab=driver-match`);
  await expect(page).toHaveURL(new RegExp(`#dsp/${dsp.id}/driver-match$`));
  await expect(link).toHaveAttribute('aria-current', 'page');
  await expect(page.getByText('4 to check', { exact: true })).toBeVisible();
  const people = page.getByRole('region', { name: 'People' });
  const card = (name: string) => people.getByRole('listitem').filter({ hasText: name });
  const check = (name: string) => card(name).getByRole('button', { name: 'Check', exact: true });
  const dialog = page.getByRole('dialog', { name: /Same person\?/ });

  await check('Ben Collins').click();
  await expect(dialog).toContainText('Ben is short for Benjamin');
  await expect(dialog).toContainText('Likely the same');
  await dialog.getByRole('button', { name: 'Same person', exact: true }).click();
  await expect(
    page.getByText('Benjamin Collins and Ben Collins are now one person', { exact: true }),
  ).toBeVisible();
  await expect(link).toContainText('3');
  await dialog.getByRole('button', { name: 'Close dialog' }).click();
  // Two people kept apart.
  await check('Drew S.').click();
  await expect(dialog).toContainText('Not sure');
  await dialog.getByRole('button', { name: 'Different people', exact: true }).click();
  await expect(page.getByText(/are kept as different people$/)).toBeVisible();
  await expect(link).toContainText('2');
  await dialog.getByRole('button', { name: 'Close dialog' }).click();
  // A name written another way is confirmed the same way.
  await check('Quinn Foster').click();
  await expect(dialog).toContainText('Similar name');
  await dialog.getByRole('button', { name: 'Same person', exact: true }).click();
  await expect(link).toContainText('1');
  await dialog.getByRole('button', { name: 'Close dialog' }).click();
  await expect(check('Quinn Foster')).toHaveCount(0);

  // Someone Amazon saw only long ago, whom Paycom doesn't list, has left and shows only under
  // Former; one Paycom has never heard of is missing from it.
  await expect(card('Taylor Brooks')).toBeVisible();
  await expect(card('Riley Quinn')).toHaveCount(0);
  const show = page.getByRole('group', { name: 'Show' });
  await show.getByRole('button', { name: /^Former/ }).click();
  await expect(card('Riley Quinn')).toContainText('Former');
  await show.getByRole('button', { name: /^Not in every system/ }).click();
  await expect(card('Dmitri Volkov')).toContainText('Not in Paycom');
  await show.getByRole('button', { name: /^All/ }).click();

  await page.getByLabel('Search people').fill('collins');
  await card('Benjamin Collins')
    .getByRole('button', { name: /^Benjamin Collins/ })
    .click();
  const sheet = page.getByRole('dialog');
  await expect(sheet).toContainText('Goes by Ben on Amazon');
  await expect(
    sheet.getByRole('listitem').filter({ hasText: 'confirmed as this person' }),
  ).toHaveCount(1);
  // A wrong link comes apart again.
  await sheet
    .locator('.driver-id')
    .filter({ hasText: 'Ben Collins' })
    .getByRole('button', { name: 'Split off', exact: true })
    .click();
  await page.getByRole('button', { name: 'Split off', exact: true }).last().click();
  await expect(page.getByText(/is now its own person/)).toBeVisible();
  await expect(sheet.locator('.driver-id')).toHaveCount(1);
  await sheet.getByRole('button', { name: 'Close dialog' }).click();
  await expect(card('Ben Collins')).toHaveCount(1);
});
