import fs from 'node:fs';
import type { Page } from '@playwright/test';
import type { DocumentsItem } from '../../api/client.js';
import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Google sends the browser straight back, as Google does once someone clicks Allow, and
// keeps the account's Drive in memory.

/**
 * Connects Google as an owner does: from Documents to the Google Drive card on Settings'
 * Connections, which Google sends the browser back to, and which sends the owner on to
 * Documents, empty.
 */
async function connect(page: Page) {
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await page.getByRole('link', { name: /Go to Settings/ }).click();
  await page.getByRole('button', { name: 'Connect Google' }).click();
  await expect(page.getByText('Nothing here yet')).toBeVisible();
}

test('large folders keep one page mounted in both layouts, search every item and reveal a new folder', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await connect(page);
  const items: DocumentsItem[] = Array.from({ length: 250 }, (_, index) => ({
    id: `large-folder-${index}`,
    name: `File ${String(index).padStart(4, '0')}.txt`,
    kind: 'file',
    size: 10,
    modifiedAt: '2026-01-05T12:00:00Z',
    modifiedBy: null,
    addedBy: null,
    items: null,
    url: `https://drive.google.com/file/d/large-folder-${index}/view`,
    location: null,
    thumbnail: null,
    extension: '.txt',
  }));
  await page.route('**/api/dsp/documents/folder*', async (route) => {
    const response = await route.fetch();
    const folder = await response.json();
    const query = new URL(route.request().url()).searchParams.get('q')?.toLowerCase() ?? '';
    await route.fulfill({
      response,
      json: { ...folder, items: items.filter((item) => item.name.toLowerCase().includes(query)) },
    });
  });
  await page.reload();
  const rows = page.locator('.documents-table tbody tr');
  await expect(rows).toHaveCount(100);
  await expect(page.locator('.row-menu .account-popover button')).toHaveCount(0);
  await page.getByLabel('Actions for File 0000.txt').click();
  await expect(page.getByRole('button', { name: 'Rename', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.locator('.row-menu .account-popover button')).toHaveCount(0);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page.getByRole('link', { name: 'File 0100.txt', exact: true })).toBeVisible();
  await expect(rows).toHaveCount(100);
  await page.getByRole('button', { name: 'Grid', exact: true }).click();
  await expect(page.locator('.documents-file-card')).toHaveCount(100);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page.locator('.documents-file-card')).toHaveCount(50);
  await page.getByLabel('Search Documents').fill('File 0249');
  await expect(page.locator('.documents-file-card')).toHaveCount(1);
  await expect(page.getByRole('link', { name: /File 0249/ })).toBeVisible();
  await page.getByLabel('Search Documents').fill('');
  await expect(page.locator('.documents-file-card')).toHaveCount(100);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByLabel('New', { exact: true }).click();
  await page.getByRole('button', { name: 'Folder', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('New folder');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.getByRole('link', { name: /New folder/ })).toBeVisible();
});

test('an owner connects Google, lands in an empty Documents and makes a folder and a Doc that opens in Google', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await connect(page);
  // Back on Settings' Connections, the code Google sent back was used once and is gone from
  // the address; the tab stays.
  await page.goBack();
  await expect(page).toHaveURL(/tab=connections/);
  expect(page.url()).not.toContain('googleCode');
  await expect(
    page.getByRole('article').filter({ hasText: 'Google Drive' }).getByRole('status'),
  ).toHaveText('Connected');
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await page.getByLabel('New').click();
  await page.getByRole('button', { name: 'Folder', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Safety & Compliance');
  await page.getByRole('button', { name: 'Create' }).click();

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
  await connect(page);

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

  // Right-clicking a file opens the same menu as its button.
  const saving = page.waitForEvent('download');
  await page.getByRole('link', { name: /Route notes week 41\.txt/ }).click({ button: 'right' });
  await page.getByRole('button', { name: 'Download' }).click();
  const saved = await saving;
  expect(saved.suggestedFilename()).toBe('Route notes week 41.txt');
});

test('an owner adds a file someone made directly in Drive from its link', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // The page's own read of Documents, once connected, names what its Drive holds out of reach.
  const overview = page.waitForResponse(
    async (response) =>
      response.url().endsWith('/api/dsp/documents') && Boolean((await response.json()).picker),
  );
  await connect(page);
  const made: { id: string; name: string }[] = (await (await overview).json()).picker.madeInDrive;
  const receipts = made.find((file) => file.name === 'Fuel receipts.pdf')!;
  await page.getByLabel('Upload', { exact: true }).click();
  await page.getByRole('button', { name: 'Drive link' }).click();
  const linking = page.getByRole('dialog', { name: 'Add a Drive link' });
  await linking
    .getByRole('textbox', { name: 'Link' })
    .fill(`https://drive.google.com/file/d/${receipts.id}/view?usp=drive_link`);
  await linking.getByRole('button', { name: 'Add' }).click();
  // Fixture mode lists its Drive's files out of reach, where Google's picker would open, with
  // the one the link names chosen.
  const picker = page.getByRole('dialog', { name: 'Add from Google Drive' });
  await expect(picker.getByLabel('Fuel receipts.pdf')).toBeChecked();
  await picker.getByRole('button', { name: 'Add 1 file' }).click();
  await expect(picker).toBeHidden();
  await expect(page.getByRole('link', { name: /Fuel receipts\.pdf/ })).toBeVisible();
});

test("Google's picker opens in the tab, signed in as the account, and comes back with what was picked", async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // Fixture mode has no picker of Google's: here the server offers one, and Google answers as
  // it does for a browser signed in to the account already.
  let account = '';
  await page.route('**/api/dsp/documents', async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    if (json.picker) {
      account = json.picker.account;
      json.picker.google = { clientId: 'client', apiKey: 'key', appId: '1' };
    }
    await route.fulfill({ response, json });
  });
  const overview = page.waitForResponse(
    async (response) =>
      response.url().endsWith('/api/dsp/documents') && Boolean((await response.json()).picker),
  );
  await page.route('https://accounts.google.com/o/oauth2/v2/auth?**', async (route) => {
    const asked = new URL(route.request().url()).searchParams;
    expect(asked.get('login_hint')).toBe(account);
    const answer = new URLSearchParams({ access_token: 'token', state: asked.get('state')! });
    await route.fulfill({
      contentType: 'text/html',
      body: `<script>location.replace(${JSON.stringify(`${asked.get('redirect_uri')}#${answer}`)})</script>`,
    });
  });
  await page.route('https://www.googleapis.com/drive/v3/about?**', (route) =>
    route.fulfill({
      headers: {
        'access-control-allow-origin': '*',
        'access-control-allow-headers': 'authorization',
      },
      json: { user: { emailAddress: account } },
    }),
  );
  // Google's picker picks what it was opened on.
  await page.route('https://apis.google.com/js/api.js', (route) =>
    route.fulfill({
      contentType: 'text/javascript',
      body: `
        window.gapi = { load: (name, options) => options.callback() };
        class DocsView {
          setIncludeFolders() { return this; }
          setSelectFolderEnabled() { return this; }
          setFileIds(ids) { this.ids = ids; return this; }
          setParent() { return this; }
        }
        class PickerBuilder {
          addView(view) { this.view = view; return this; }
          enableFeature() { return this; }
          setOAuthToken(token) { this.token = token; return this; }
          setDeveloperKey() { return this; }
          setAppId() { return this; }
          setCallback(callback) { this.callback = callback; return this; }
          build() {
            return { setVisible: () => this.token === 'token' && this.callback({
              action: 'picked', docs: this.view.ids.split(',').map((id) => ({ id })),
            }) };
          }
        }
        window.google = { picker: { DocsView, PickerBuilder, ViewId: {}, Feature: {},
          Action: { PICKED: 'picked', CANCEL: 'cancel' } } };
      `,
    }),
  );
  await connect(page);
  const made: { id: string; name: string }[] = (await (await overview).json()).picker.madeInDrive;
  const receipts = made.find((file) => file.name === 'Fuel receipts.pdf')!;
  await page.getByLabel('Upload', { exact: true }).click();
  await page.getByRole('button', { name: 'Drive link' }).click();
  const linking = page.getByRole('dialog', { name: 'Add a Drive link' });
  await linking
    .getByRole('textbox', { name: 'Link' })
    .fill(`https://drive.google.com/file/d/${receipts.id}/view?usp=drive_link`);
  await linking.getByRole('button', { name: 'Add' }).click();
  await expect(page.getByText('Added to Documents')).toBeVisible();
  await expect(page.getByRole('link', { name: /Fuel receipts\.pdf/ })).toBeVisible();
  expect(page.context().pages()).toHaveLength(1);
});
