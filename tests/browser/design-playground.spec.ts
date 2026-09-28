import { test, expect, login } from './fixtures.js';

test.use({ viewport: { width: 1920, height: 1080 } });

test('health failures and recovery preserve an open canvas and its state', async ({ page }) => {
  await login(page);
  const link = page.getByRole('link', { name: 'Design Playground', exact: true });
  await expect(link).toBeVisible();
  const origin = new URL(page.url()).origin;
  let running = true;
  let starts = 0;
  await page.route('**/api/platform/design-playground/status', (route) =>
    route.fulfill({ json: { configured: true, running, canStart: true } }),
  );
  await page.route('**/api/platform/design-playground/start', (route) => {
    starts++;
    running = true;
    return route.fulfill({ json: { configured: true, running, canStart: true } });
  });
  await page.route('**/api/platform/design-playground', (route) =>
    route.fulfill({ json: { origin, ticket: 'fixture-ticket', expiresAt: Date.now() + 30000 } }),
  );
  await page.route(origin + '/', (route) =>
    route.request().frame() === page.mainFrame()
      ? route.continue()
      : route.fulfill({
          contentType: 'text/html',
          body: '<!doctype html><title>Fixture canvas</title><input aria-label="Draft">',
        }),
  );
  await link.click();
  const canvas = page.frameLocator('iframe[title="Design Playground"]');
  const draft = canvas.getByRole('textbox', { name: 'Draft' });
  await draft.fill('Keep this unsaved canvas state');
  running = false;
  await expect(page.getByRole('region', { name: 'Playground recovery' })).toBeVisible({
    timeout: 15000,
  });
  await expect(draft).toHaveValue('Keep this unsaved canvas state');
  await page.getByRole('button', { name: 'Start playground', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Playground recovery' })).toHaveCount(0);
  await expect(draft).toHaveValue('Keep this unsaved canvas state');
  expect(starts).toBe(1);
});
