import path from 'node:path';
import { test, expect } from '@playwright/test';
import { draw } from '../support/harness.js';

test('a watcher that fails stops alone, logged by its ID, and one opened offline loads as the browser comes back online', async ({
  page,
  context,
}) => {
  const logged: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') logged.push(message.text());
  });

  // The page opens offline, the late watcher's code failing to download while it is.
  let offline = true;
  await context.setOffline(true);
  await draw(
    page,
    path.resolve(import.meta.dirname, '../support/watchers.tsx'),
    (file) => offline && file.startsWith('/assets/late-watcher-'),
  );
  await expect(page.getByText('Page', { exact: true })).toBeVisible();

  // Back online, each watcher's code is asked for: one whose code never loads is logged by its ID.
  offline = false;
  await context.setOffline(false);
  await expect(page.getByText('Late watcher running')).toBeVisible();
  await expect(page.getByText('Steady watcher running')).toBeVisible();
  await expect.poll(() => logged.some((line) => line.includes('missing.loads'))).toBe(true);

  // One failing as it runs leaves the page and the others running.
  await page.getByRole('button', { name: 'Fail' }).click();
  await expect(page.getByRole('button', { name: 'Fail' })).toHaveCount(0);
  await expect(page.getByText('Page', { exact: true })).toBeVisible();
  await expect(page.getByText('Steady watcher running')).toBeVisible();
  await expect.poll(() => logged.some((line) => line.includes('failing.runs'))).toBe(true);
});
