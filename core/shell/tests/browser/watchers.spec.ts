import path from 'node:path';
import { test, expect } from '@playwright/test';
import { draw } from '../support/harness.js';

test('a watcher that fails stops alone, logged by its ID, and one whose code failed to load loads as the browser comes back online', async ({
  page,
  context,
}) => {
  const logged: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') logged.push(message.text());
  });
  await draw(page, path.resolve(import.meta.dirname, '../support/watchers.tsx'));
  await expect(page.getByText('Steady watcher running')).toBeVisible();
  await expect.poll(() => logged.some((line) => line.includes('late.loads'))).toBe(true);
  await expect(page.getByText('Late watcher running')).toHaveCount(0);

  // One failing as it runs leaves the page and the others running.
  await page.getByRole('button', { name: 'Fail' }).click();
  await expect(page.getByRole('button', { name: 'Fail' })).toHaveCount(0);
  await expect(page.getByText('Page', { exact: true })).toBeVisible();
  await expect(page.getByText('Steady watcher running')).toBeVisible();
  await expect.poll(() => logged.some((line) => line.includes('failing.runs'))).toBe(true);

  // Its code is asked for again as the browser comes back online.
  await context.setOffline(true);
  await context.setOffline(false);
  await expect(page.getByText('Late watcher running')).toBeVisible();
});
