import path from 'node:path';
import { test, expect } from '@playwright/test';
import { draw } from '../support/harness.js';

test("the app's list rests in view when it's shown again just as Settings' finishes sliding in", async ({
  page,
}) => {
  await draw(page, path.resolve(import.meta.dirname, '../support/sidebar-panels.tsx'));
  const home = page.getByRole('link', { name: 'Home page', exact: true });
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('link', { name: 'Profile', exact: true })).toBeVisible();

  // The slide ends as the address leaves Settings, the two drawn in one render: as when the
  // platform owner exits a DSP's view from its Settings while the page is busy.
  await page.evaluate(() => {
    for (const moving of document.getAnimations()) moving.finish();
    location.hash = '#home';
  });
  // Once the slide back ends, Settings' list gone, the app's stays where it came in.
  await expect(page.getByText('Profile', { exact: true })).toHaveCount(0);
  await expect(home).toBeInViewport();
});
