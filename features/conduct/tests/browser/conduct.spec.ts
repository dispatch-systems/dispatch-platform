import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import type { Page } from '@playwright/test';

async function newUniform(page: Page) {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Conduct', exact: true }).click();
  await page.getByRole('button', { name: 'New writeup' }).click();
  await page.getByRole('combobox', { name: 'Driver', exact: true }).click();
  await page.getByRole('listbox', { name: 'Drivers' }).getByRole('option').first().click();
  const name = await page.getByRole('combobox', { name: 'Driver', exact: true }).inputValue();
  await page.getByLabel('Category', { exact: true }).selectOption({ label: 'Uniform' });
  await expect(
    page.getByRole('textbox', { name: /Statement of the problem: missing item/ }),
  ).toBeVisible();
  return name;
}
async function text(page: Page, label: string, value: string) {
  const section = page
    .locator('[data-conduct-section]')
    .filter({ has: page.getByText(label, { exact: true }) });
  const edit = section.getByRole('button', { name: 'Edit text', exact: true });
  if (await edit.count()) await edit.click();
  await page.getByRole('textbox', { name: label, exact: true }).fill(value);
}
async function complete(page: Page) {
  await page
    .getByRole('textbox', { name: /Statement of the problem: missing item/ })
    .fill('a safety vest');
  await page.getByRole('textbox', { name: /Statement of the problem: date/ }).fill('11/08/2026');
}

test('a driver is written up, list filters survive opening it, and signing locks it', async ({
  page,
}) => {
  const name = await newUniform(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Fill in the blanks marked in yellow.');
  await expect(
    page.getByRole('textbox', { name: /Statement of the problem: missing item/ }),
  ).toBeFocused();
  await complete(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
  await page.getByRole('button', { name: 'Conduct', exact: true }).click();
  await page.getByRole('tab', { name: 'Writeups', exact: true }).click();
  await page.getByRole('searchbox', { name: 'Search drivers' }).fill(name);
  await page.getByRole('combobox', { name: 'Category', exact: true }).selectOption('uniform');
  await page.getByRole('button', { name: 'Awaiting signature', exact: true }).click();
  await page.locator('tbody tr').filter({ hasText: name }).getByRole('button').first().click();
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
  await page.getByRole('button', { name: 'Conduct', exact: true }).click();
  await expect(page.getByRole('searchbox', { name: 'Search drivers' })).toHaveValue(name);
  await expect(page.getByRole('combobox', { name: 'Category', exact: true })).toHaveValue(
    'uniform',
  );
  await expect(page.getByRole('combobox', { name: 'Status' })).toHaveValue('waiting');
  await page
    .locator('tbody tr')
    .filter({ hasText: name })
    .getByRole('button', { name: 'Mark signed' })
    .click();
  await page.getByRole('dialog').getByRole('button', { name: 'Mark signed', exact: true }).click();
  // Signing removes it from the waiting queue. It remains available under all statuses.
  await expect(page.getByText('No writeups', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Awaiting signature', exact: true }).click();
  await page.locator('tbody tr').filter({ hasText: name }).getByRole('button').first().click();
  await expect(page.getByText(/^Signed \d+\/\d+\/\d{4}$/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Edit', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Print', exact: true })).toBeVisible();
});

test('browser Back keeps the dirty writeup until one Save or Discard decision', async ({
  page,
}) => {
  await newUniform(page);
  await complete(page);
  await page.goBack();
  const prompt = page.getByRole('dialog', { name: 'Save changes?' });
  await expect(prompt).toBeVisible();
  await expect(page.getByRole('heading', { name: 'New writeup', exact: true })).toBeVisible();
  await prompt.getByRole('button', { name: 'Discard changes' }).click();
  await expect(page.getByRole('heading', { name: 'Conduct', exact: true })).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'New writeup', exact: true })).toHaveCount(0);
});

test('a readable long notice prints whole on one letter page', async ({ page }) => {
  await newUniform(page);
  const long = 'DA did not arrive to work in full uniform and was reminded at stand up. '.repeat(6);
  await text(page, '1. Statement of the problem', long);
  const entries = Array.from({ length: 5 }, (_, n) => `Incident ${n + 1}: late arrival discussed.`);
  await text(page, '2. Prior discussion or warnings', entries.join('\n'));
  await text(page, '3. Corrective action', long);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Awaiting signature', { exact: true })).toBeVisible();
  await page.evaluate(() => {
    window.print = () => {};
  });
  await page.getByRole('button', { name: 'Print', exact: true }).click();
  const sheet = page.locator('.conduct-print .conduct-notice');
  await expect(sheet).toBeAttached();
  await page.emulateMedia({ media: 'print' });
  const answers = await sheet.locator('.conduct-notice-answer').evaluateAll((boxes) =>
    boxes.map((box) => ({
      fit: Number(getComputedStyle(box).getPropertyValue('--fit')),
      whole: box.scrollHeight <= box.clientHeight + 1,
    })),
  );
  expect(answers).toHaveLength(3);
  for (const answer of answers) {
    expect(answer.whole).toBe(true);
    expect(answer.fit).toBeLessThan(1);
    expect(answer.fit * 10.3).toBeGreaterThanOrEqual(7);
  }
  await expect(sheet.locator('[data-overflow],[data-unreadable]')).toHaveCount(0);
  const pdf = (await page.pdf({ preferCSSPageSize: true })).toString('latin1');
  expect(pdf.match(/\/Type\s*\/Page[^s]/g)).toHaveLength(1);
});

test('unreadable multiline content remains saved whole and is refused for printing', async ({
  page,
}) => {
  await newUniform(page);
  await complete(page);
  const entries = Array.from(
    { length: 25 },
    (_, n) => `Incident ${n + 1}: late arrival discussed.`,
  ).join('\n');
  await text(page, '2. Prior discussion or warnings', entries);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Awaiting signature', { exact: true })).toBeVisible();
  await page.evaluate(() => {
    window.print = () => {
      throw new Error('Unreadable notice reached print');
    };
  });
  await page.getByRole('button', { name: 'Print', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Shorten the notice sections before printing.');
  await expect(page.locator('.conduct-paper .conduct-notice-answer').nth(1)).toHaveText(entries);
  await expect(page.locator('.conduct-print')).toHaveCount(0);
});

test('cancelling photo preparation sends neither an upload nor a signature', async ({ page }) => {
  await newUniform(page);
  await complete(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByRole('button', { name: 'Mark signed', exact: true }).click();
  const png = await page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.width = 2;
    canvas.height = 2;
    return canvas.toDataURL('image/png').split(',')[1]!;
  });
  await page.evaluate(() => {
    const state = window as typeof window & {
      conductPhotoTest: { release?: () => void; closed: boolean; sent: number };
    };
    state.conductPhotoTest = { closed: false, sent: 0 };
    const original = window.createImageBitmap.bind(window);
    Object.defineProperty(window, 'createImageBitmap', {
      value: async (image: Blob) => {
        await new Promise<void>((resolve) => {
          state.conductPhotoTest.release = resolve;
        });
        const bitmap = await original(image);
        const close = bitmap.close.bind(bitmap);
        Object.defineProperty(bitmap, 'close', {
          value: () => {
            close();
            state.conductPhotoTest.closed = true;
          },
        });
        return bitmap;
      },
    });
    const send = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function (body) {
      state.conductPhotoTest.sent++;
      send.call(this, body);
    };
  });
  await page
    .getByLabel(/^Signed copy/)
    .setInputFiles({
      name: 'paper.png',
      mimeType: 'image/png',
      buffer: Buffer.from(png, 'base64'),
    });
  await expect(page.getByRole('img', { name: 'Selected signed copy' })).toBeVisible();
  await page.getByRole('dialog').getByRole('button', { name: 'Mark signed', exact: true }).click();
  await page.waitForFunction(() =>
    Boolean(
      (window as typeof window & { conductPhotoTest: { release?: () => void } }).conductPhotoTest
        .release,
    ),
  );
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.evaluate(() =>
    (
      window as typeof window & { conductPhotoTest: { release: () => void } }
    ).conductPhotoTest.release(),
  );
  await page.waitForFunction(
    () =>
      (window as typeof window & { conductPhotoTest: { closed: boolean } }).conductPhotoTest.closed,
  );
  expect(
    await page.evaluate(
      () =>
        (window as typeof window & { conductPhotoTest: { sent: number } }).conductPhotoTest.sent,
    ),
  ).toBe(0);
  await expect(page.getByText('Awaiting signature', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Edit', exact: true })).toBeVisible();
});
