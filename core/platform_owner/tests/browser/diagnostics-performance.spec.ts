import fs from 'node:fs';
import { test, expect, login } from '../../../shell/tests/support/fixtures.js';

test('Diagnostics scopes read errors and retries to the active tab', async ({ page }) => {
  await page.route('**/api/platform/jobs', (route) => route.fulfill({ json: [] }));
  await page.route('**/api/platform/health', (route) =>
    route.fulfill({
      status: 400,
      json: { error: 'synthetic_health_failure', message: 'Synthetic health failure' },
    }),
  );
  await login(page);
  await page.getByRole('link', { name: 'Diagnostics', exact: true }).click();
  await expect(page.getByRole('alert')).toHaveText('Synthetic health failure');
  await expect(page.getByRole('button', { name: 'Retry loading', exact: true })).toBeVisible();
  await expect(page.locator('.loading')).toHaveCount(0);
  await page.getByRole('tab', { name: /^Collections/ }).click();
  await expect(
    page.getByRole('heading', { name: 'No collections yet', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Retry loading', exact: true })).toHaveCount(0);
  await page.getByRole('tab', { name: /^Email/ }).click();
  await expect(page.getByRole('alert')).toHaveText('Synthetic health failure');
  await expect(page.getByRole('button', { name: 'Retry loading', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Test DSPs', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Test DSPs', exact: true })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('retained Diagnostics keeps its tab while a different platform page loads', async ({
  page,
}) => {
  await login(page);
  await page.getByRole('link', { name: 'Diagnostics', exact: true }).click();
  const emailTab = page.getByRole('tab', { name: /^Email/ });
  await emailTab.click();
  await expect(page.getByRole('region', { name: 'Email delivery', exact: true })).toBeVisible();
  let release!: () => void;
  const hold = new Promise<void>((resolve) => {
    release = resolve;
  });
  const asset = JSON.parse(fs.readFileSync('.build/tooling/build-info.json', 'utf8'))
    .dashboardAssets['../../core/platform_owner/frontend/audit/index.ts'].file as string;
  await page.route('**/' + asset, async (route) => {
    await hold;
    await route.continue();
  });
  try {
    // Programmatic navigation avoids loading the destination through pointer intent first.
    await page
      .getByRole('link', { name: 'Audit log', exact: true })
      .evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page.getByText('Opening page…', { exact: true })).toBeVisible();
    await expect(page.locator('#main-content')).toHaveAttribute('aria-busy', 'true');
    await expect(page.getByRole('tab', { name: /^Email/, includeHidden: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await expect(
      page.getByRole('region', { name: 'Email delivery', exact: true, includeHidden: true }),
    ).toBeVisible();
    release();
    await expect(page.getByRole('heading', { name: 'Audit log', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Diagnostics', exact: true })).toHaveCount(0);
  } finally {
    release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});
