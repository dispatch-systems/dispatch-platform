import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';

test('a member arranges the sidebar where it is, with the keyboard, by dragging and by hiding, and finds it so on coming back', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const names = page
    .getByRole('navigation', { name: 'Primary navigation' })
    .locator('.nav-item > span:first-of-type');
  await expect(names.last()).toHaveText('Settings');
  const [first, second, third, fourth, ...rest] = await names.allTextContents();
  await page.getByLabel('Account menu').click();
  await page.getByRole('button', { name: 'Edit sidebar' }).click();
  const handle = (name: string) => page.getByRole('button', { name: `Move ${name}` });

  await handle(second!).focus();
  await page.keyboard.press('ArrowUp');
  await expect(page.getByRole('button', { name: /^Move / }).first()).toHaveAccessibleName(
    `Move ${second}`,
  );
  // The first, now second, is dragged below the third.
  const centre = async (name: string) => {
    const box = (await handle(name).boundingBox())!;
    return [box.x + box.width / 2, box.y + box.height / 2] as const;
  };
  await page.mouse.move(...(await centre(first!)));
  await page.mouse.down();
  await page.mouse.move(...(await centre(third!)), { steps: 8 });
  await page.mouse.up();
  const kept = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/sidebar') &&
      response.request().postDataJSON().hidden.length === 1,
  );
  await page.getByRole('button', { name: `Hide ${fourth}` }).click();
  await expect(page.getByRole('button', { name: `Show ${fourth}` })).toBeVisible();
  await page.getByRole('button', { name: 'Done' }).click();

  const arranged = [second!, third!, first!, ...rest];
  await expect(names).toHaveText(arranged);
  expect((await kept).status()).toBe(200);
  await page.reload();
  await expect(names).toHaveText(arranged);
});
