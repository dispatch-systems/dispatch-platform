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
