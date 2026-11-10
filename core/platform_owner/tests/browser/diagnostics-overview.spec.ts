import { test, expect, login } from '../../../shell/tests/support/fixtures.js';

test('the runtime release stays inside its tile on desktop and phone', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const { release } = await owner.read('/api/platform/health');
  expect(release).toMatch(/^[a-f0-9]{64}$/);
  await login(page);
  await page.getByRole('link', { name: 'Diagnostics', exact: true }).click();
  const runtime = page.getByRole('region', { name: 'Platform runtime', exact: true });
  await expect(runtime).toContainText(release);
  await page.evaluate(() => document.fonts.ready);
  for (const [width, height] of [
    [1920, 1080],
    [390, 844],
  ] as const) {
    await page.setViewportSize({ width, height });
    expect(
      await runtime.locator('small').evaluate((element) => {
        const container = element.getBoundingClientRect();
        const text = document.createRange();
        text.selectNodeContents(element);
        return [...text.getClientRects()].every(
          (line) => line.left >= container.left && line.right <= container.right,
        );
      }),
    ).toBe(true);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
});
