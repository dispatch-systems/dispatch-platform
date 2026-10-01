import { test, expect, login } from './fixtures.js';

test('the platform owner makes a key, sees it once, tests it, changes and revokes it', async ({
  page,
}) => {
  await login(page);
  await page.getByRole('link', { name: 'Agents', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Agents', level: 1 })).toBeVisible();
  await expect(page.getByText('No keys yet')).toBeVisible();

  await page.getByRole('button', { name: 'New key', exact: true }).click();
  const sheet = page.getByRole('dialog', { name: 'New key' });
  await sheet.getByLabel('Name').fill('Laptop – Claude Code');
  await sheet.getByText('Choose DSPs', { exact: true }).click();
  await sheet.getByRole('checkbox', { name: 'Northline Logistics' }).check();
  await expect(sheet.getByRole('radio', { name: /Read only/ })).toBeChecked();
  await sheet.getByRole('button', { name: 'Create key', exact: true }).click();

  // The key is shown this once, with its setup and a working test.
  const ready = page.getByRole('dialog', { name: 'Laptop – Claude Code is ready' });
  const token = await ready.locator('.agents-key code').textContent();
  expect(token).toMatch(/^dsk_dev_[0-9A-Za-z]{38}$/);
  await expect(ready.locator('.agents-snippet')).toContainText(`DISPATCH_KEY="${token}"`);
  await ready.getByRole('button', { name: 'Send test request' }).click();
  await expect(ready.getByRole('status')).toContainText('Connected · Read only · 1 DSP');
  await page.screenshot({ path: test.info().outputPath('agents-ready.png') });
  await ready.getByRole('button', { name: 'Done', exact: true }).click();

  const row = page.getByRole('row').filter({ hasText: 'Laptop – Claude Code' });
  await expect(row).toContainText(`…${token!.slice(-4)}`);
  await expect(row).toContainText('Read only');
  await expect(row).toContainText('Northline Logistics');
  // The page never shows the key again.
  await expect(page.getByText(token!)).toHaveCount(0);

  // A changed key asks before its edits are thrown away.
  await row.getByRole('button', { name: 'Open Laptop – Claude Code' }).click();
  const edit = page.getByRole('dialog', { name: 'Laptop – Claude Code' });
  await edit.getByText('Operator', { exact: true }).click();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Save changes', exact: true }).last().click();
  await expect(row).toContainText('Operator');

  // Testing a key from the Connect tab answers as an agent would.
  await page.getByRole('tab', { name: 'Connect', exact: true }).click();
  const connect = page.getByRole('region', { name: 'Agents and scripts' });
  await connect.getByLabel('Key to test').fill(token!);
  await connect.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(connect.getByRole('status')).toContainText('Connected · Operator · 1 DSP');

  // Revoked, it stops at once.
  await page.getByRole('tab', { name: 'Keys', exact: true }).click();
  await row.getByRole('button', { name: 'Open Laptop – Claude Code' }).click();
  await page.getByRole('button', { name: 'Revoke key', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Revoke Laptop – Claude Code?' })
    .getByRole('button', { name: 'Revoke key', exact: true })
    .click();
  await expect(page.getByRole('button', { name: 'Show 1 revoked or expired key' })).toBeVisible();
  await page.getByRole('tab', { name: 'Connect', exact: true }).click();
  await connect.getByLabel('Key to test').fill(token!);
  await connect.getByRole('button', { name: 'Test key', exact: true }).click();
  await expect(connect.getByRole('status')).toContainText('That key was revoked.');
});
