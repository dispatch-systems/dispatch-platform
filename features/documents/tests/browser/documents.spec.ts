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
  await page.addInitScript(() => {
    const durations: number[] = [];
    const observer = new PerformanceObserver((list) =>
      durations.push(...list.getEntries().map((entry) => entry.duration)),
    );
    observer.observe({ entryTypes: ['longtask'] });
    Object.assign(window, { documentsPerformance: { durations, observer } });
  });
  const measurements: {
    layout: string;
    nodes: number;
    longTasks: number;
    maxLongTaskMs: number;
  }[] = [];
  const measure = async (layout: string) => {
    const measured = await page.evaluate(() => {
      const { durations, observer } = (
        window as typeof window & {
          documentsPerformance: { durations: number[]; observer: PerformanceObserver };
        }
      ).documentsPerformance;
      const tasks = [...durations, ...observer.takeRecords().map((entry) => entry.duration)];
      durations.length = 0;
      return {
        nodes: document.querySelectorAll('*').length,
        longTasks: tasks.length,
        maxLongTaskMs: Math.max(0, ...tasks),
      };
    });
    measurements.push({ layout, ...measured });
    expect(measured.nodes).toBeLessThan(10_000);
  };
  const fileCount = 1500;
  try {
    await login(page);
    await openDsp(page, 'Northline Logistics');
    await connect(page);
    const items: DocumentsItem[] = Array.from({ length: fileCount }, (_, index) => ({
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
      shortcut: false,
      unavailable: false,
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
    const cards = page.locator('.documents-file-card');
    await expect(cards).toHaveCount(100);
    await measure('initial-grid');
    await page.getByRole('button', { name: 'List', exact: true }).click();
    const rows = page.locator('.documents-table tbody tr');
    await expect(rows).toHaveCount(100);
    await measure('list');
    await expect(page.locator('.documents-table .row-menu .account-popover button')).toHaveCount(0);
    await page.getByLabel('Actions for File 0000.txt').click();
    await expect(page.getByRole('button', { name: 'Rename', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('.documents-table .row-menu .account-popover button')).toHaveCount(0);
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    await expect(page.getByRole('link', { name: 'File 0100.txt', exact: true })).toBeVisible();
    await expect(rows).toHaveCount(100);
    await page.getByRole('button', { name: 'Grid', exact: true }).click();
    await expect(page.locator('.documents-file-card')).toHaveCount(100);
    await measure('grid');
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
    await measure('mobile-grid');
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    await expect(page.locator('.documents-file-card')).toHaveCount(100);
    await page.getByLabel('Search Documents').fill('File 1499');
    await expect(page.locator('.documents-file-card')).toHaveCount(1);
    await expect(page.getByRole('link', { name: /File 1499/ })).toBeVisible();
    await page.getByLabel('Search Documents').fill('');
    await expect(page.locator('.documents-file-card')).toHaveCount(100);
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    await page.getByRole('button', { name: 'Next', exact: true }).click();
    await page.getByLabel('New', { exact: true }).click();
    await page.getByRole('button', { name: 'Folder', exact: true }).click();
    await page.getByRole('textbox', { name: 'Name' }).fill('New folder');
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('link', { name: /New folder/ })).toBeVisible();
  } finally {
    const output = test.info().outputPath('documents-performance.json');
    fs.writeFileSync(
      output,
      JSON.stringify(
        {
          files: fileCount,
          pageSize: 100,
          measurements,
          note: 'Observed long tasks between interaction checkpoints; CI host load affects timings.',
        },
        null,
        2,
      ),
    );
    await test.info().attach('documents-performance', {
      path: output,
      contentType: 'application/json',
    });
  }
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
  // Settings holds the sidebar while it's open: Back leaves it for the app's pages.
  await page.getByRole('link', { name: 'Back', exact: true }).click();
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
  // The list shows each upload's size.
  await page.getByRole('button', { name: 'List', exact: true }).click();
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

test('a member drags files into folders and up the path, moves several at once, and undoes a move', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await connect(page);
  for (const name of ['EOS Sheets', 'Archive']) {
    await page.getByLabel('New').click();
    await page.getByRole('button', { name: 'Folder', exact: true }).click();
    await page.getByRole('textbox', { name: 'Name' }).fill(name);
    await page.getByRole('button', { name: 'Create' }).click();
  }
  await page.getByLabel('Files to upload').setInputFiles(
    ['October EOS.pdf', 'November EOS.pdf'].map((name) => ({
      name,
      mimeType: 'application/pdf',
      buffer: Buffer.from('%PDF-1.4'),
    })),
  );
  await page
    .getByRole('dialog', { name: 'Upload 2 files' })
    .getByRole('button', { name: 'Upload' })
    .click();
  const october = page.getByRole('link', { name: /October EOS\.pdf/ });
  const november = page.getByRole('link', { name: /November EOS\.pdf/ });
  const sheets = page.getByRole('link', { name: /^EOS Sheets/ });
  await expect(november).toBeVisible();

  // Dragged onto a folder, a file goes into it; Undo brings it back.
  await october.dragTo(sheets);
  await expect(october).toHaveCount(0);
  await expect(sheets).toContainText('1 item');
  await page
    .getByRole('status')
    .filter({ hasText: 'Moved to EOS Sheets' })
    .getByRole('button', { name: 'Undo' })
    .click();
  await expect(october).toBeVisible();
  await expect(sheets).toContainText('0 items');

  // Ctrl or ⌘ selects instead of opening, and Move to… moves what's selected.
  await october.click({ modifiers: ['ControlOrMeta'] });
  await november.click({ modifiers: ['ControlOrMeta'] });
  await page.getByLabel('Actions for November EOS.pdf').click();
  await page.getByRole('button', { name: 'Move to…' }).click();
  const moving = page.getByRole('dialog', { name: 'Move 2 items' });
  await moving.getByRole('button', { name: 'EOS Sheets' }).click();
  await moving.getByRole('button', { name: 'Move here' }).click();
  await expect(sheets).toContainText('2 items');
  await expect(november).toHaveCount(0);

  // Inside a folder, a file dragged onto a step of the path goes up to it.
  await sheets.click();
  await expect(november).toBeVisible();
  await october.dragTo(
    page.getByRole('navigation', { name: 'Folder' }).getByRole('link', { name: 'Documents' }),
  );
  await expect(october).toHaveCount(0);
  await page
    .getByRole('navigation', { name: 'Folder' })
    .getByRole('link', { name: 'Documents' })
    .click();
  await expect(october).toBeVisible();

  // A file from the computer dropped on a folder goes up into it.
  await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(['Checklist'], 'Checklist.txt', { type: 'text/plain' }));
    const folder = [...document.querySelectorAll('.documents-folder-card')].find((card) =>
      card.textContent?.startsWith('Archive'),
    )!;
    for (const type of ['dragover', 'drop'])
      folder.dispatchEvent(
        new DragEvent(type, { dataTransfer: transfer, bubbles: true, cancelable: true }),
      );
  });
  await page
    .getByRole('dialog', { name: 'Upload file' })
    .getByRole('button', { name: 'Upload' })
    .click();
  await expect(page.getByRole('link', { name: /^Archive/ })).toContainText('1 item');
  await expect(page.getByRole('link', { name: /Checklist\.txt/ })).toHaveCount(0);
});

test('a file the Documents account reaches comes in from its link at once', async ({ page }) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // The page's own read of Documents, once connected, names what its Drive holds outside it.
  const overview = page.waitForResponse(
    async (response) =>
      response.url().endsWith('/api/dsp/documents') && Boolean((await response.json()).picker),
  );
  await connect(page);
  const made: { id: string; name: string }[] = (await (await overview).json()).picker.elsewhere;
  const map = made.find((file) => file.name === 'Route map 2025.png')!;
  await page.getByLabel('Upload', { exact: true }).click();
  await page.getByRole('button', { name: 'Drive link' }).click();
  const linking = page.getByRole('dialog', { name: 'Add a Drive link' });
  await linking
    .getByRole('textbox', { name: 'Link' })
    .fill(`https://drive.google.com/file/d/${map.id}/view?usp=drive_link`);
  await linking.getByRole('button', { name: 'Add' }).click();
  // The account that holds Documents reaches it, so no picker opens.
  await expect(page.getByText('Added to Documents')).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Add from Google Drive' })).toBeHidden();
  await expect(page.getByRole('link', { name: /Route map 2025\.png/ })).toBeVisible();
});

test("a file only the member reaches comes in through Google's picker, signed in as them, in the tab", async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  // Fixture mode has no picker of Google's: here the server offers one, and Google answers as
  // it does for a browser signed in to the member already.
  let me = '';
  let into = '';
  await page.route('**/api/dsp/documents', async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    if (json.picker) {
      me = json.picker.me;
      into = json.picker.folder;
      json.picker.google = { clientId: 'client', apiKey: 'key', appId: '1' };
    }
    await route.fulfill({ response, json });
  });
  // The account that holds Documents doesn't reach the file the link names, the first time.
  let refused = false;
  await page.route('**/api/dsp/documents/add', async (route) => {
    if (refused) return route.continue();
    refused = true;
    await route.fulfill({ status: 409, json: { error: 'documents_picked_elsewhere' } });
  });
  const overview = page.waitForResponse(
    async (response) =>
      response.url().endsWith('/api/dsp/documents') && Boolean((await response.json()).picker),
  );
  await page.route('https://accounts.google.com/o/oauth2/v2/auth?**', async (route) => {
    const asked = new URL(route.request().url()).searchParams;
    expect(asked.get('login_hint')).toBe(me);
    const answer = new URLSearchParams({ access_token: 'token', state: asked.get('state')! });
    await route.fulfill({
      contentType: 'text/html',
      body: `<script>location.replace(${JSON.stringify(`${asked.get('redirect_uri')}#${answer}`)})</script>`,
    });
  });
  // The member owns it, so it moves, as them, into the folder open in Documents.
  const moved: string[] = [];
  await page.route('https://www.googleapis.com/drive/v3/files/**', async (route) => {
    const cors = {
      'access-control-allow-origin': '*',
      'access-control-allow-headers': 'authorization,content-type',
      'access-control-allow-methods': 'GET,PATCH,POST',
    };
    if (route.request().method() === 'OPTIONS')
      return route.fulfill({ status: 204, headers: cors });
    if (route.request().method() === 'PATCH') moved.push(route.request().url());
    await route.fulfill({ headers: cors, json: { ownedByMe: true, parents: ['root'] } });
  });
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
  const made: { id: string; name: string }[] = (await (await overview).json()).picker.elsewhere;
  const map = made.find((file) => file.name === 'Route map 2025.png')!;
  await page.getByLabel('Upload', { exact: true }).click();
  await page.getByRole('button', { name: 'Drive link' }).click();
  const linking = page.getByRole('dialog', { name: 'Add a Drive link' });
  await linking
    .getByRole('textbox', { name: 'Link' })
    .fill(`https://drive.google.com/file/d/${map.id}/view?usp=drive_link`);
  await linking.getByRole('button', { name: 'Add' }).click();
  await expect(page.getByText('Added to Documents')).toBeVisible();
  await expect(page.getByRole('link', { name: /Route map 2025\.png/ })).toBeVisible();
  expect(page.context().pages()).toHaveLength(1);
  expect(moved).toHaveLength(1);
  expect(moved[0]).toContain(`addParents=${into}`);
});
