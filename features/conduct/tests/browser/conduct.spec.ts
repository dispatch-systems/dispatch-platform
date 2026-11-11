import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import type { Page } from '@playwright/test';

async function newUniform(page: Page) {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Conduct', exact: true }).click();
  return uniform(page);
}
async function uniform(page: Page) {
  await page.getByRole('button', { name: 'New writeup' }).click();
  await page.getByRole('combobox', { name: 'Driver', exact: true }).click();
  await page.getByRole('listbox', { name: 'Drivers' }).getByRole('option').first().click();
  const name = await page.getByRole('combobox', { name: 'Driver', exact: true }).inputValue();
  await page
    .getByRole('combobox', { name: 'Category', exact: true })
    .selectOption({ label: 'Uniform' });
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

test('a driver is written up from the page, then marked signed and locked', async ({ page }) => {
  const name = await newUniform(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Fill in the blanks marked in yellow.');
  await expect(
    page.getByRole('textbox', { name: /Statement of the problem: missing item/ }),
  ).toBeFocused();
  await complete(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  const revised = 'DA arrived without a safety vest and was issued a replacement.';
  await text(page, '1. Statement of the problem', revised);
  const detail = /\/api\/dsp\/conduct\/writeups\/[^/?]+$/;
  await page.route(detail, (route) =>
    route.request().method() === 'GET'
      ? route.fulfill({ status: 503, json: { error: 'unavailable' } })
      : route.continue(),
  );
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
  await expect(page.locator('.conduct-paper .conduct-notice-answer').first()).toHaveText(revised);
  await page.evaluate(() => {
    window.print = () => {};
  });
  await page.getByRole('button', { name: 'Print', exact: true }).click();
  await expect(page.locator('.conduct-print .conduct-notice-answer').first()).toHaveText(revised);
  await page.unroute(detail);
  await page.getByRole('button', { name: 'Conduct', exact: true }).click();
  await page.getByRole('tab', { name: 'Writeups', exact: true }).click();
  await page.getByRole('textbox', { name: 'Search drivers' }).fill(name);
  await page.getByRole('combobox', { name: 'Category', exact: true }).selectOption('uniform');
  await page.getByRole('button', { name: 'Awaiting signature', exact: true }).click();
  await page.locator('tbody tr').filter({ hasText: name }).getByRole('button').first().click();
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible();
  await page.getByRole('button', { name: 'Conduct', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Search drivers' })).toHaveValue(name);
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

test('returning an edited notice to its date restores untouched suggestions', async ({ page }) => {
  await newUniform(page);
  await complete(page);
  const original = await page.getByRole('textbox', { name: 'Date', exact: true }).inputValue();
  const prior = await page
    .getByRole('textbox', { name: '2. Prior discussion or warnings' })
    .inputValue();
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByRole('button', { name: 'Conduct', exact: true }).click();
  await uniform(page);
  await expect(page.getByRole('radio', { name: /Written.*Suggested/i })).toBeChecked();
  await complete(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.getByRole('radio', { name: /Written.*Suggested/i })).toBeChecked();
  const correction = 'Check the uniform with dispatch before starting the route.';
  await text(page, '3. Corrective action', correction);
  const [month, day, year] = original.split('/').map(Number);
  const previous = new Date(Date.UTC(year!, month! - 1, day! - 1)).toLocaleDateString('en-US', {
    timeZone: 'UTC',
  });
  const date = page.getByRole('textbox', { name: 'Date', exact: true });
  await date.fill(previous);
  await date.press('Enter');
  await expect(page.getByRole('radio', { name: /Verbal.*Suggested/i })).toBeChecked();
  await expect(page.getByRole('textbox', { name: '2. Prior discussion or warnings' })).toHaveValue(
    prior,
  );
  await date.fill(original);
  await date.press('Enter');
  await expect(page.getByRole('radio', { name: /Written.*Suggested/i })).toBeChecked();
  await expect(page.getByRole('textbox', { name: '2. Prior discussion or warnings' })).toHaveValue(
    /Previous writeups for Uniform: Verbal Warning/,
  );
  await expect(page.getByRole('textbox', { name: '3. Corrective action' })).toHaveValue(correction);
});

test('leaving Conduct while its new-writeup read waits keeps the destination address', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Conduct', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Conduct', exact: true })).toBeVisible();
  let release!: () => void, requested!: () => void;
  const waiting = new Promise<void>((resolve) => (release = resolve));
  const started = new Promise<void>((resolve) => (requested = resolve));
  await page.route('**/api/dsp/conduct', async (route) => {
    const response = await route.fetch();
    requested();
    await waiting;
    await route.fulfill({ response });
  });
  await page.getByRole('button', { name: 'New writeup' }).click();
  await started;
  await page.getByRole('link', { name: 'Documents', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Documents', exact: true })).toBeVisible();
  const destination = page.url();
  const response = page.waitForResponse((response) => response.url().endsWith('/api/dsp/conduct'));
  release();
  await response;
  // Let the released read's continuation render before checking the page it no longer owns.
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  await expect(page).toHaveURL(destination);
  await expect(page.getByRole('heading', { name: 'Documents', exact: true })).toBeVisible();
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

test('a long writeup still prints on one letter page, its sections shrunk to fit their lines', async ({
  page,
}) => {
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

test('photo preparation cancels safely, then a rotated copy is signed', async ({ page }) => {
  await newUniform(page);
  await complete(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByRole('button', { name: 'Mark signed', exact: true }).click();
  const png = await page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.width = 2;
    canvas.height = 3;
    return canvas.toDataURL('image/png').split(',')[1]!;
  });
  await page.evaluate(() => {
    const state = window as typeof window & {
      conductPhotoTest: {
        release?: () => void;
        closed: boolean;
        sent: number;
        restore: () => void;
      };
    };
    state.conductPhotoTest = { closed: false, sent: 0, restore: () => {} };
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
    state.conductPhotoTest.restore = () => {
      window.createImageBitmap = original;
      XMLHttpRequest.prototype.send = send;
    };
    XMLHttpRequest.prototype.send = function (body) {
      state.conductPhotoTest.sent++;
      send.call(this, body);
    };
  });
  const photoFile = {
    name: 'paper.png',
    mimeType: 'image/png',
    buffer: Buffer.from(png, 'base64'),
  };
  await page.getByLabel(/^Signed copy/).setInputFiles(photoFile);
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
  await page.evaluate(() =>
    (
      window as typeof window & { conductPhotoTest: { restore: () => void } }
    ).conductPhotoTest.restore(),
  );
  await page.getByRole('button', { name: 'Mark signed', exact: true }).click();
  await page.getByLabel(/^Signed copy/).setInputFiles(photoFile);
  await page.getByRole('button', { name: 'Rotate', exact: true }).click();
  let uploads = 0,
    signatures = 0;
  await page.route('**/api/dsp/conduct/writeups/*/copy?*', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    uploads++;
    if (uploads === 1) {
      await route.fetch();
      await route.abort('connectionreset');
    } else await route.continue();
  });
  await page.route('**/api/dsp/conduct/writeups/*/sign', async (route) => {
    signatures++;
    if (signatures === 1) {
      await route.fetch();
      await route.abort('connectionreset');
    } else await route.continue();
  });
  await page.getByRole('dialog').getByRole('button', { name: 'Mark signed', exact: true }).click();
  const retry = page.getByRole('dialog');
  await expect(retry.getByRole('alert')).toBeVisible();
  await expect(retry.getByRole('button', { name: 'Rotate', exact: true })).toBeDisabled();
  await retry.getByRole('button', { name: 'Mark signed', exact: true }).click();
  await expect(retry.getByRole('alert')).toBeVisible();
  await retry.getByRole('button', { name: 'Mark signed', exact: true }).click();
  await expect(page.getByText(/^Signed \d+\/\d+\/\d{4}$/)).toBeVisible();
  expect(uploads).toBe(2);
  expect(signatures).toBe(2);
  const copy = page.getByRole('img', { name: 'Signed copy', exact: true });
  await expect(copy).toBeVisible();
  await expect
    .poll(() =>
      copy.evaluate((image: HTMLImageElement) => [image.naturalWidth, image.naturalHeight]),
    )
    .toEqual([3, 2]);
});
