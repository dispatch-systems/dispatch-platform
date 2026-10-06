import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';
import { totp, virtualAuthenticator } from '../../../core/accounts/tests/support/authenticator.js';

// Core's Security tab, as a DSP's Settings shows it too: the Settings feature's page.

test('DSP settings keep recovery codes up until they are saved', async ({ page }) => {
  await virtualAuthenticator(page);
  // Reloading the session reopens the DSP view and remounts its settings page, so it
  // must wait until the one-time codes are saved.
  let sessionLoads = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/session') sessionLoads += 1;
  });
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Security', exact: true }).click();
  const recoveryHeading = page.getByRole('heading', { name: 'Save your recovery codes' });
  const keepsCodesUntilSaved = async (enroll: () => Promise<void>) => {
    sessionLoads = 0;
    await enroll();
    await expect(recoveryHeading).toBeVisible();
    await expect(page.getByLabel('Formatted recovery codes')).toContainText('Code 10: ');
    expect(sessionLoads).toBe(0);
    await page.getByRole('button', { name: 'I saved my recovery codes' }).click();
    await expect(recoveryHeading).not.toBeVisible();
    await expect.poll(() => sessionLoads).toBe(1);
  };

  await keepsCodesUntilSaved(async () => {
    await page.getByRole('button', { name: 'Add passkey', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add passkey' });
    await dialog.getByLabel('Passkey name').fill('DSP account key');
    await dialog.getByRole('button', { name: 'Add passkey', exact: true }).click();
  });
  await expect(page.getByText('DSP account key', { exact: true })).toBeVisible();
  await keepsCodesUntilSaved(() =>
    page.getByRole('button', { name: 'Replace recovery codes', exact: true }).click(),
  );

  await page.getByRole('button', { name: 'Remove', exact: true }).click();
  await page.getByRole('button', { name: 'Turn off', exact: true }).click();
  await expect(page.getByText('DSP account key', { exact: true })).not.toBeVisible();
  await keepsCodesUntilSaved(async () => {
    await page.getByRole('button', { name: 'Add app', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add authenticator app' });
    const secret = (await dialog.locator('code').textContent())!;
    await dialog.getByLabel('6-digit code').fill(totp(secret));
    await dialog.getByRole('button', { name: 'Verify and add', exact: true }).click();
  });
  await expect(page.getByRole('button', { name: 'Remove', exact: true })).toBeVisible();

  // Leaving the page with the codes up still brings the session up to date.
  sessionLoads = 0;
  await page.getByRole('button', { name: 'Replace recovery codes', exact: true }).click();
  await expect(recoveryHeading).toBeVisible();
  await page.goBack();
  await expect(recoveryHeading).not.toBeVisible();
  await expect.poll(() => sessionLoads).toBe(1);
});
