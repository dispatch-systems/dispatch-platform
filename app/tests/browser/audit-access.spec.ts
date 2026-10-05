import fs from 'node:fs';
import type { Page } from '@playwright/test';
import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';
import { dspHash, platformHash } from '../../../core/shell/frontend/runtime/navigation.js';

// The audit log is the platform owner's alone, whichever way into a DSP's Settings once led to
// it: a test across the platform owner's dashboard and Settings.

const item = (page: Page, text: string | RegExp) =>
  page.getByRole('listitem').filter({ hasText: text });

test('audit access is only in the Platform Owner Dashboard, including old DSP links', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const dspId = new URL(page.url()).hash.split('/')[1]!;
  await page.goto(dspHash(dspId, 'settings', { tab: 'audit' }));
  await expect(page.getByRole('tab', { name: 'Profile', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expect(page.getByRole('tab', { name: 'Audit log', exact: true })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Audit log', exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Search activity')).toHaveCount(0);
  for (const theme of ['light', 'dark']) {
    await page.evaluate(
      (value) => document.documentElement.setAttribute('data-theme', value),
      theme,
    );
  }
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.setViewportSize({ width: 1440, height: 1000 });

  await page.getByRole('button', { name: 'Exit view', exact: true }).click();
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Profile', exact: true })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Platform support', exact: true })).toHaveCount(0);
  await page.getByRole('link', { name: 'Audit log', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Audit log', exact: true })).toBeVisible();
  await expect(item(page, 'created Summit Delivery')).toHaveCount(1);
  await page.getByLabel('DSP', { exact: true }).selectOption({ label: 'Northline Logistics' });
  await expect(item(page, 'created Summit Delivery')).toHaveCount(0);
  await expect(item(page, 'Platform Owner opened Northline Logistics').first()).toBeVisible();

  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  const csv = fs.readFileSync(await (await download).path(), 'utf8');
  expect(csv).toContain('Northline Logistics');
  expect(csv).not.toContain('Summit Delivery');
  await page.getByLabel('DSP', { exact: true }).selectOption('');
  await expect(item(page, 'Platform Owner exported the audit log')).toContainText(/\d+ events?/, {
    timeout: 15000,
  });
});

test('DSP members cannot open the platform audit page', async ({ page }) => {
  await login(page, 'member@dispatch.test');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Audit log', exact: true })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Audit log', exact: true })).toHaveCount(0);
  await page.goto(platformHash('audit'));
  await expect(page.getByRole('heading', { name: 'Your DSPs', exact: true })).toBeVisible();
  await expect(page.getByLabel('Search activity')).toHaveCount(0);
});
