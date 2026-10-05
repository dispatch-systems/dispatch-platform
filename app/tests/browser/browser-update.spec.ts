import { test, expect } from '../../../core/shell/tests/support/fixtures.js';
import { clockVisible, loginWithClock } from '../../../core/server/tests/support/update.js';
import { expectDate, setDate } from '../../../features/timecard/tests/support/page.js';

// An automatic update reloads the page in place: a test of Timecard's state across core's reload.

test('reload preserves DSP, meal tab, selected date and search on mobile', async ({
  page,
  dispatch,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  let ready = false;
  let loads = 0;
  page.on('load', () => loads++);
  await page.route('**/api/browser-update', (route) =>
    route.fulfill({ json: { build: 'c'.repeat(64), ready } }),
  );
  await loginWithClock(page);
  await page.evaluate((id) => {
    location.hash = `dsp/${id}/paycom`;
  }, dsp.id);
  await expect(page.getByRole('heading', { name: 'Timecard', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Meal Breaks', exact: true }).click();
  await setDate(page, '2026-09-15');
  await page.getByLabel('Search meal break employees').fill('Avery');
  await page.evaluate(() => window.scrollTo(0, 200));
  const scroll = await page.evaluate(() => window.scrollY);
  expect(scroll).toBeGreaterThan(0);
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  const before = loads;
  ready = true;
  await page.clock.runFor(35000);
  await page.clock.runFor(500);
  await expect
    .poll(async () => {
      await page.clock.runFor(250);
      return loads;
    })
    .toBe(before + 1);
  await clockVisible(page, page.getByRole('tab', { name: 'Meal Breaks', exact: true }));
  await expect(page).toHaveURL(new RegExp(`dsp/${dsp.id}/paycom`));
  await expect(page.getByRole('tab', { name: 'Meal Breaks', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await expectDate(page, '2026-09-15');
  await expect(page.getByLabel('Search meal break employees')).toHaveValue('Avery');
  await expect
    .poll(async () => {
      await page.clock.runFor(100);
      return page.evaluate(() => window.scrollY);
    })
    .toBe(scroll);
});
