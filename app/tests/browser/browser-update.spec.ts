import { test, expect } from '../../../core/shell/tests/support/fixtures.js';
import {
  clockVisible,
  loginWithClock,
  runAnswered,
} from '../../../core/server/tests/support/update.js';
import { expectDate, setDate } from '../../../features/timecard/tests/support/page.js';

test.use({ dispatchOptions: { env: { DISPATCH_FIXTURE_TODAY: '2026-09-15' } } });

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
  const filtered = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return (
      url.pathname === '/api/dsp/paycom/meal-breaks/page' &&
      url.searchParams.get('date') === '2026-09-15' &&
      url.searchParams.get('q') === 'avery'
    );
  });
  await page.getByLabel('Search meal break employees').fill('Avery');
  expect((await (await filtered).json()).rows.length).toBeGreaterThan(0);
  await expect(page.getByRole('region', { name: 'Meal break comparison' })).toBeVisible();
  await expect(page.locator('.paycom-day-results')).toHaveAttribute('aria-busy', 'false');
  await page.evaluate(() => window.scrollTo(0, 200));
  const scroll = await page.evaluate(() => window.scrollY);
  expect(scroll).toBeGreaterThan(0);
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  const before = loads;
  ready = true;
  await runAnswered(page, 35000);
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
