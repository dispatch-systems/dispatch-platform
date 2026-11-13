import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';

test('a member arranges the sidebar where it is, with the keyboard, by dragging and by hiding, and finds it so on coming back', async ({
  page,
}) => {
  await page.clock.install();
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const names = page
    .getByRole('navigation', { name: 'Primary navigation' })
    .locator('.nav-item > span:first-of-type');
  // The DSP's own page is drawn once its view is in, and the sidebar lists every page then.
  await expect(page.locator('.dsp-page')).toBeVisible();
  await expect(names.last()).toHaveText('Settings');
  const [home, first, second, third, fourth, ...rest] = await names.allTextContents();
  await page.getByLabel('Account menu').click();
  await page.getByRole('button', { name: 'Edit sidebar' }).click();
  const handle = (name: string) => page.getByRole('button', { name: `Move ${name}` });
  // The page the DSP opens on stays first, and Settings last: neither has a handle.
  await expect(handle(second!)).toBeVisible();
  await expect(handle(home!)).toHaveCount(0);
  await expect(handle('Settings')).toHaveCount(0);

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
  // What dragging runs on lets a click through again 50 ms after a drop, sooner than a hand clicks.
  await page.clock.runFor(100);
  const kept = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/sidebar') &&
      response.request().postDataJSON().hidden.length === 1,
  );
  await page.getByRole('button', { name: `Hide ${fourth}` }).click();
  await expect(page.getByRole('button', { name: `Show ${fourth}` })).toBeVisible();
  await page.getByRole('button', { name: 'Done' }).click();

  const arranged = [home!, second!, third!, first!, ...rest];
  await expect(names).toHaveText(arranged);
  expect((await kept).status()).toBe(200);
  await page.reload();
  await expect(names).toHaveText(arranged);
});
