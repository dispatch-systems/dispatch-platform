import { test, expect, login, openDsp } from './fixtures.js';

const enter = async (page: Parameters<typeof login>[0]) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
};

test('a failed DSP list stops loading and retries without restarting the session', async ({
  page,
}) => {
  let failed = true;
  let sessions = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/session') sessions++;
  });
  await page.route('**/api/platform/dsps', (route) =>
    failed
      ? route.fulfill({ status: 403, json: { error: 'permission_denied' } })
      : route.continue(),
  );
  await login(page);
  await expect(page.getByRole('button', { name: 'Retry loading', exact: true })).toBeVisible();
  await expect(page.locator('.loading')).toHaveCount(0);
  const before = sessions;
  failed = false;
  await page.getByRole('button', { name: 'Retry loading', exact: true }).click();
  await expect(page.getByRole('region', { name: 'DSPs', exact: true })).toBeVisible();
  expect(sessions).toBe(before);
});

test('an uncached offline page pauses visibly and loads after reconnect', async ({ page }) => {
  let failed = true;
  await page.route('**/api/dsp/timecards?**', (route) =>
    failed
      ? route.fulfill({ status: 403, json: { error: 'permission_denied' } })
      : route.continue(),
  );
  await enter(page);
  await page.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(
    page.getByText('Your role does not allow this action.', { exact: true }),
  ).toBeVisible();
  await page.getByRole('link', { name: 'Home Page', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Currently under development', exact: true }),
  ).toBeVisible();
  await page.context().setOffline(true);
  await page.getByRole('link', { name: 'Timecard', exact: true }).click();
  await expect(
    page.getByText('You’re offline. This page will load when you reconnect.'),
  ).toBeVisible();
  await expect(page.locator('.loading')).toHaveCount(0);
  failed = false;
  await page.context().setOffline(false);
  await expect(page.locator('.paycom-timecard-table tbody tr').first()).toBeVisible();
});

test('Settings reads only counts for its badge and settled connections avoid fast polling', async ({
  page,
}) => {
  await page.clock.install();
  const paths: string[] = [];
  page.on('request', (request) => paths.push(new URL(request.url()).pathname));
  await enter(page);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect
    .poll(() => paths.filter((path) => path === '/api/dsp/driver-match/counts').length)
    .toBeGreaterThan(0);
  expect(paths.filter((path) => path === '/api/dsp/driver-match')).toHaveLength(0);
  await page.getByRole('tab', { name: 'Connections', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Paycom', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Cortex', exact: true })).toBeVisible();
  const count = () =>
    paths.filter(
      (path) => path === '/api/dsp/connections' || path === '/api/dsp/connections/cortex',
    ).length;
  const before = count();
  await page.clock.runFor(12_000);
  expect(count()).toBe(before);
});

test('leaving Employees discards queued warmups while shared active requests may finish', async ({
  page,
}) => {
  await page.clock.install();
  let release!: () => void;
  const hold = new Promise<void>((resolve) => {
    release = resolve;
  });
  let requests = 0;
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.startsWith('/api/dsp/employees/')) requests++;
  });
  let routed = 0;
  await page.route('**/api/dsp/employees/*', async (route) => {
    routed++;
    if (routed > 1) await hold;
    await route.continue().catch(() => {});
  });
  try {
    await enter(page);
    await page.getByRole('link', { name: 'Timecard', exact: true }).click();
    await page.getByRole('tab', { name: 'Employees', exact: true }).click();
    await expect(page.locator('.employee-timecard-section')).toHaveAttribute('aria-busy', 'false');
    await expect.poll(() => requests).toBeGreaterThan(1);
    await page.getByRole('link', { name: 'Home Page', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: 'Currently under development', exact: true }),
    ).toBeVisible();
    const before = requests;
    release();
    await page.unrouteAll({ behavior: 'wait' });
    await page.clock.runFor(1000);
    expect(requests).toBe(before);
  } finally {
    release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});
