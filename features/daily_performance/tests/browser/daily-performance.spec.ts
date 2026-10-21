import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission.

test('Daily Performance has its switch on the DSPs page, on for the seeded DSP', async ({
  page,
}) => {
  await login(page);
  await page
    .getByRole('region', { name: 'DSPs', exact: true })
    .getByRole('button', { name: /Northline Logistics/ })
    .click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  await expect(pane.getByRole('switch', { name: 'Daily Performance', exact: true })).toBeChecked();
});

test('its settings are set with its collection on Collectors, where searching Settings finds them', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  // It has no page of its own.
  const pages = page.getByRole('navigation', { name: 'Settings', exact: true });
  await expect(pages.getByRole('link', { name: 'Collectors', exact: true })).toBeVisible();
  await expect(pages.getByRole('link', { name: 'Daily Performance', exact: true })).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Search settings', exact: true }).fill('look back');
  const found = page.getByRole('listbox', { name: 'Found', exact: true }).getByRole('option');
  await expect(found).toHaveCount(1);
  await expect(found).toContainText('Collectors');
  await page.keyboard.press('Enter');
  // Collectors opens on its collection, the section pointed out.
  await expect(page).toHaveURL(/tab=collectors&collection=daily_performance$/);
  const section = page
    .getByRole('region', { name: 'Daily Performance', exact: true })
    .locator('[data-settings-section="daily_performance.collection"]');
  await expect(section).toHaveAttribute('data-found');
  await section.getByLabel('Look back', { exact: true }).fill('3');
  await section.getByRole('button', { name: 'Save', exact: true }).click();
  await expect
    .poll(
      async () =>
        (await owner.read('/api/dsp/settings')).sections.find(
          (each: { id: string }) => each.id === 'daily_performance.collection',
        ).values.lookback_days,
    )
    .toBe(3);
});
