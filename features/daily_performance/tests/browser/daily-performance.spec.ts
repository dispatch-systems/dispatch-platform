import { test, expect, login } from '../../../../core/shell/tests/support/fixtures.js';

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
  await pane
    .getByRole('navigation', { name: 'Feature areas' })
    .getByRole('button', { name: /^Daily Performance/ })
    .click();
  await expect(
    pane.getByRole('switch', { name: 'Daily Performance page', exact: true }),
  ).toBeChecked();
});
