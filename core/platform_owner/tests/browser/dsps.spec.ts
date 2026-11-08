import { test, expect, login, demo } from '../../../shell/tests/support/fixtures.js';

test('long after signing in, disabling a DSP asks the owner to confirm it’s them, then goes through', async ({
  page,
  dispatch,
}) => {
  await login(page);
  await page
    .getByRole('region', { name: 'DSPs', exact: true })
    .getByRole('button', { name: /Summit Delivery/ })
    .click();
  // Signed in by now: the list shows only to a session.
  dispatch.database('data/platform/accounts.sqlite', (db) => {
    db.prepare('UPDATE sessions SET created_at=0').run();
    db.prepare('UPDATE session_security SET verified_at=0, password_verified_at=0').run();
  });
  const detail = page.getByRole('region', { name: 'Summit Delivery' });
  await detail.getByLabel('Actions for Summit Delivery').click();
  await page.getByRole('button', { name: 'Disable DSP', exact: true }).click();
  const disable = page.getByRole('dialog', { name: 'Disable Summit Delivery?' });
  await disable.getByRole('button', { name: 'Disable DSP', exact: true }).click();
  const confirm = page.getByRole('dialog', { name: 'Confirm it’s you' });
  await confirm.getByLabel('Password', { exact: true }).fill(demo.password);
  await confirm.getByRole('button', { name: 'Verify password', exact: true }).click();
  // Verified, the DSP is disabled without asking again.
  await expect(page.getByText('DSP disabled', { exact: true })).toBeVisible();
  await expect(confirm).toHaveCount(0);
  await expect(disable).toHaveCount(0);
  await expect(detail.getByRole('heading', { name: /Disabled/ })).toBeVisible();
});

test('the table shows a count’s DSPs, opens one across the page, another from beside its name, and Back returns to them all', async ({
  page,
}) => {
  await login(page);
  const table = page.getByRole('region', { name: 'DSPs', exact: true });
  // A count shows only its DSPs.
  const counts = page.getByRole('group', { name: 'DSP summary' });
  await counts.getByRole('button', { name: /Online/ }).click();
  await expect(table.getByRole('button', { name: /Dev DSP/ })).toBeVisible();
  await expect(table.getByRole('button', { name: /Northline Logistics/ })).toHaveCount(0);
  await counts.getByRole('button', { name: /DSPs/ }).click();
  await table.getByRole('button', { name: /Northline Logistics/ }).click();
  const northline = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  // A DSP opens on its features.
  await expect(northline.getByRole('tab', { name: 'Features', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(table).toHaveCount(0);

  await northline.getByRole('button', { name: 'Switch DSP', exact: true }).click();
  const switcher = page.getByRole('dialog', { name: 'Switch DSP' });
  await switcher.getByLabel('Search DSPs').fill('Summit');
  await switcher.getByRole('button', { name: /Summit Delivery/ }).click();
  await expect(page.getByRole('region', { name: 'Summit Delivery', exact: true })).toBeVisible();
  await expect(switcher).toHaveCount(0);

  await page.goBack();
  await expect(northline).toBeVisible();
  await page.goBack();
  await expect(table.getByRole('button', { name: /Summit Delivery/ })).toBeVisible();
  await expect(northline).toHaveCount(0);
});
