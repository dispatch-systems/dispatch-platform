import { test, expect } from '../../../shell/tests/support/fixtures.js';
import { exports, open } from '../support/audit-log.js';

test('audit search pauses old actions and exports until the visible filter is applied', async ({
  page,
}) => {
  await page.clock.install();
  const requests = await open(page);
  const results = page.locator('.audit-results');
  const exportButton = page.getByRole('button', { name: 'Export', exact: true });
  await expect(results).toHaveAttribute('aria-busy', 'false');
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  await page.getByLabel('Search activity').fill('role');
  await expect(results).toHaveAttribute('aria-busy', 'true');
  await expect(results).toHaveAttribute('inert', '');
  await expect(exportButton).toBeDisabled();
  expect(exports).toHaveLength(0);
  expect(requests.at(-1)?.get('q')).toBeNull();
  await page.clock.runFor(249);
  expect(requests.at(-1)?.get('q')).toBeNull();
  await page.clock.runFor(1);
  await expect.poll(() => requests.at(-1)?.get('q')).toBe('role');
  await expect(results).toHaveAttribute('aria-busy', 'false');
  await expect(results).not.toHaveAttribute('inert');
  await expect(exportButton).toBeEnabled();
  await page.clock.resume();
  const download = page.waitForEvent('download');
  await exportButton.click();
  await download;
  expect(exports).toHaveLength(1);
  expect(exports[0]!.get('q')).toBe('role');
});

test('changing an expanded audit filter requests only the first page of the new filter', async ({
  page,
}) => {
  const requests = await open(page);
  await page.getByRole('button', { name: 'Load more', exact: true }).click();
  await expect.poll(() => requests.at(-1)?.get('limit')).toBe('100');
  await expect(page.locator('.audit-results')).toHaveAttribute('aria-busy', 'false');
  await page
    .getByRole('group', { name: 'Area' })
    .getByRole('button', { name: /^Roles/ })
    .click();
  await expect.poll(() => requests.at(-1)?.get('area')).toBe('roles');
  await expect(page.locator('.audit-results')).toHaveAttribute('aria-busy', 'false');
  expect(
    requests.filter((query) => query.get('area') === 'roles').map((query) => query.get('limit')),
  ).toEqual(['50']);
});

test('changing audit filters offline retains inert results and resumes after reconnect', async ({
  page,
}) => {
  await page.clock.install();
  const requests = await open(page);
  await expect(page.locator('.audit-results')).toHaveAttribute('aria-busy', 'false');
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  const before = requests.length;
  const status = page.locator('.audit-log').getByRole('status');
  await page.context().setOffline(true);
  try {
    await page.getByLabel('Search activity').fill('role');
    await expect(status).toHaveText(
      'Activity loading is paused. It will resume when this page is visible and you’re online.',
    );
    await expect(page.locator('.audit-results')).toHaveAttribute('inert', '');
    await expect(page.locator('.audit-results')).toHaveAttribute('aria-busy', 'false');
    await expect(page.getByRole('button', { name: 'Export', exact: true })).toBeDisabled();
    expect(requests).toHaveLength(before);
    // Wait for the debounced filter, which must not initiate a read while offline.
    await page.clock.runFor(300);
    expect(requests).toHaveLength(before);
    await page.context().setOffline(false);
    await expect.poll(() => requests.at(-1)?.get('q')).toBe('role');
    await expect(page.locator('.audit-results')).not.toHaveAttribute('inert');
    await expect(status).toHaveCount(0);
  } finally {
    await page.context().setOffline(false);
    await page.clock.resume();
  }
});
