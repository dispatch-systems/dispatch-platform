import fs from 'node:fs';
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

test('a member names files as they upload, drops one on the page, renames and downloads one', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await page.getByRole('button', { name: 'Connect Google' }).click();
  await page
    .getByRole('dialog', { name: 'Documents is ready' })
    .getByRole('button', { name: 'Skip' })
    .click();

  await page.getByLabel('Files to upload').setInputFiles([
    { name: 'Uniform policy.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4') },
    { name: 'Route notes.txt', mimeType: 'text/plain', buffer: Buffer.from('Route notes') },
  ]);
  // Each is named before it goes up, its extension kept out of reach.
  const naming = page.getByRole('dialog', { name: 'Upload 2 files' });
  const policy = naming.getByRole('textbox', { name: 'Name of Uniform policy.pdf' });
  await expect(policy).toHaveValue('Uniform policy');
  await policy.fill('Uniform policy 2026');
  await naming.getByRole('button', { name: 'Upload' }).click();
  const uploads = page.getByRole('region', { name: 'Uploads' });
  await expect(uploads).toContainText('2 uploaded');
  await expect(page.getByRole('link', { name: /Uniform policy 2026\.pdf/ })).toBeVisible();
  await expect(page.getByRole('row', { name: /Route notes\.txt/ })).toContainText('11 B');

  // Too large a file never leaves the browser. The file is sparse: its size, not its bytes.
  const large = test.info().outputPath('Dashcam.mp4');
  fs.writeFileSync(large, '');
  fs.truncateSync(large, 100 * 1024 * 1024 + 1);
  await page.getByLabel('Files to upload').setInputFiles(large);
  const tooLarge = page.getByRole('dialog', { name: 'Upload file' });
  await expect(tooLarge).toContainText('Files can be up to 100 MB.');
  await expect(tooLarge.getByRole('button', { name: 'Upload' })).toBeDisabled();
  await tooLarge.getByRole('button', { name: 'Cancel' }).click();
  await uploads.getByRole('button', { name: 'Close uploads' }).click();

  // Dropped on the page, a file goes up to the folder open.
  await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(['Checklist'], 'Checklist.txt', { type: 'text/plain' }));
    const zone = document.querySelector('.documents-drop')!;
    for (const type of ['dragover', 'drop'])
      zone.dispatchEvent(
        new DragEvent(type, { dataTransfer: transfer, bubbles: true, cancelable: true }),
      );
  });
  await page
    .getByRole('dialog', { name: 'Upload file' })
    .getByRole('button', { name: 'Upload' })
    .click();
  await expect(uploads).toContainText('1 uploaded');
  await expect(page.getByRole('link', { name: /Checklist\.txt/ })).toBeVisible();

  // Renaming keeps the extension out of reach too.
  await page.getByLabel('Actions for Route notes.txt').click();
  await page.getByRole('button', { name: 'Rename' }).click();
  const renaming = page.getByRole('dialog', { name: 'Rename file' });
  await expect(renaming.getByRole('textbox', { name: 'Name' })).toHaveValue('Route notes');
  await renaming.getByRole('textbox', { name: 'Name' }).fill('Route notes week 41');
  await renaming.getByRole('button', { name: 'Rename' }).click();

  const saving = page.waitForEvent('download');
  await page.getByLabel('Actions for Route notes week 41.txt').click();
  await page.getByRole('button', { name: 'Download' }).click();
  const saved = await saving;
  expect(saved.suggestedFilename()).toBe('Route notes week 41.txt');
});

test('an owner adds a file someone made directly in Drive', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await page.getByRole('button', { name: 'Connect Google' }).click();
  await page
    .getByRole('dialog', { name: 'Documents is ready' })
    .getByRole('button', { name: 'Skip' })
    .click();
  // Fixture mode lists its Drive's files out of reach, where Google's picker would open.
  await page.getByLabel('New').click();
  await page.getByRole('button', { name: /Add from Google Drive/ }).click();
  const picker = page.getByRole('dialog', { name: 'Add from Google Drive' });
  await picker.getByLabel('Fuel receipts.pdf').check();
  await picker.getByRole('button', { name: 'Add 1 file' }).click();
  await expect(picker).toBeHidden();
  await expect(page.getByRole('link', { name: /Fuel receipts\.pdf/ })).toBeVisible();
});
