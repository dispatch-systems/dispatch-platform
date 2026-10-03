import type { Page } from '@playwright/test';
import type { DvicStatus } from '../../../../shared/contracts/dvic.js';
import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';
import { seedDvic } from '../support/dvic.js';

const gate = () => {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
};
const enter = async (page: Page, name: string) => {
  await login(page);
  await openDsp(page, name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
};

test('DVIC loads the selected week once, publishes complete cursors and reuses visited weeks @paint-budget', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch, 501);
  const status = gate();
  const secondPage = gate();
  let statusWaiting = false;
  let cursorWaiting = false;
  const requests: string[] = [];
  await page.route('**/api/dsp/dvic/status', async (route) => {
    statusWaiting = true;
    await status.promise;
    await route.continue();
  });
  await page.route('**/api/dsp/dvic/inspections?**', async (route) => {
    requests.push(route.request().url());
    if (new URL(route.request().url()).searchParams.has('after')) {
      cursorWaiting = true;
      await secondPage.promise;
    }
    await route.continue();
  });
  try {
    await enter(page, dsp.name);
    await expect.poll(() => statusWaiting).toBe(true);
    expect(requests).toEqual([]);
    status.release();
    await expect.poll(() => cursorWaiting).toBe(true);
    await expect(page.getByRole('group', { name: 'Inspection week' })).toContainText(
      'Sep 20 – Sep 26',
    );
    await expect(page.locator('.dvic-footer')).toHaveCount(0);
    secondPage.release();
    await page.getByRole('tab', { name: 'Week', exact: true }).click();
    await expect(page.locator('.dvic-footer')).toContainText('501 records this week');
    expect(requests).toHaveLength(2);
    expect(requests.every((url) => new URL(url).searchParams.get('from') === '2026-09-20')).toBe(
      true,
    );
    await page.getByRole('button', { name: 'Previous week', exact: true }).click();
    await expect(page.getByText('No short inspections stored', { exact: true })).toBeVisible();
    const before = requests.length;
    await page.getByRole('button', { name: 'Latest', exact: true }).click();
    await expect(page.locator('.dvic-footer')).toContainText('501 records this week');
    await page.getByRole('link', { name: 'Home Page', exact: true }).click();
    const paint = await page.getByRole('link', { name: 'DVIC', exact: true }).evaluate((link) => {
      const started = performance.now();
      return new Promise<{ text: string; ms: number }>((resolve) => {
        window.addEventListener(
          'hashchange',
          () =>
            requestAnimationFrame(() =>
              resolve({
                text: document.querySelector('.dvic-footer')?.textContent ?? '',
                ms: performance.now() - started,
              }),
            ),
          { once: true },
        );
        (link as HTMLAnchorElement).click();
      });
    });
    expect(paint.text).toContain('501 records this week');
    expect(paint.ms).toBeLessThan(100);
    test.info().annotations.push({ type: 'cached DVIC paint ms', description: String(paint.ms) });
    await expect(page.getByRole('tab', { name: 'Week', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await expect(page.locator('.dvic-footer')).toContainText('501 records this week');
    expect(requests).toHaveLength(before);
  } finally {
    status.release();
    secondPage.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('large DVIC weeks bound rendered drivers while totals, paging, details and filters remain complete', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch, 501);
  dispatch.database(`dsps/${dsp.id}/data/dvic/dvic.sqlite`, (db) =>
    db.exec(
      `UPDATE dvic_inspections SET transporter_id='driver-'||inspection_key,transporter_name='Driver '||inspection_key`,
    ),
  );
  await enter(page, dsp.name);
  await page.getByRole('tab', { name: 'Week', exact: true }).click();
  await expect(page.locator('.dvic-footer')).toContainText('501 records this week');
  const rows = page.locator('.dvic-grid tbody tr');
  await expect(rows).toHaveCount(25);
  const totals = await page.locator('.dvic-grid thead th small').allTextContents();
  expect(totals.reduce((sum, value) => sum + Number(value), 0)).toBe(501);
  const paging = page.getByRole('navigation', { name: 'Inspection drivers', exact: true });
  await expect(paging).toContainText('1–25 of 501');
  await paging.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(rows).toHaveCount(25);
  await expect(paging).toContainText('26–50 of 501');
  await page.locator('.dvic-cell').first().click();
  await expect(page.getByRole('dialog')).toContainText('Driver page-row-');
  await page.keyboard.press('Escape');
  await page.getByRole('link', { name: 'Home Page', exact: true }).click();
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await expect(paging).toContainText('26–50 of 501');
  await page.getByRole('textbox', { name: 'Search drivers', exact: true }).fill('page-row-500');
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText('Driver page-row-500');
  await expect(page.locator('.dvic-footer')).toContainText('1 record this week');
  await page.getByRole('link', { name: 'Home Page', exact: true }).click();
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Search drivers', exact: true })).toHaveValue(
    'page-row-500',
  );
  await expect(rows).toHaveCount(1);
});

test('DVIC active polling accepts a status read that takes longer than its five-second interval', async ({
  page,
  dispatch,
}) => {
  await page.clock.install();
  const { dsp } = await seedDvic(dispatch);
  const hold = gate();
  let requests = 0;
  let waiting = false;
  let aborted = false;
  page.on('requestfailed', (request) => {
    if (waiting && new URL(request.url()).pathname === '/api/dsp/dvic/status') aborted = true;
  });
  await page.route('**/api/dsp/dvic/status', async (route) => {
    requests++;
    const response = await route.fetch();
    const body = (await response.json()) as DvicStatus;
    if (requests === 1) body.jobs[0]!.status = 'running';
    else {
      waiting = true;
      await hold.promise;
    }
    await route.fulfill({ response, json: body });
  });
  try {
    await enter(page, dsp.name);
    await expect(page.getByRole('button', { name: 'Syncing…', exact: true })).toBeDisabled();
    await page.clock.fastForward(5000);
    await expect.poll(() => waiting).toBe(true);
    // Cross another complete polling interval while the second read is held.
    await page.clock.fastForward(6000);
    expect(aborted).toBe(false);
    expect(requests).toBe(2);
    hold.release();
    await expect(page.getByRole('button', { name: 'Sync now', exact: true })).toBeEnabled();
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('a newly published DVIC status replaces retained weeks and supersedes older reads', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch);
  const status = gate();
  const oldWeek = gate();
  let changed = false;
  let waiting = false;
  let reads = 0;
  await page.route('**/api/dsp/dvic/status', async (route) => {
    const response = await route.fetch();
    const body = (await response.json()) as DvicStatus;
    if (changed) {
      await status.promise;
      for (const week of body.weeks)
        week.checkedAt = new Date(Date.parse(week.checkedAt) + 1000).toISOString();
    }
    await route.fulfill({ response, json: body });
  });
  await page.route('**/api/dsp/dvic/inspections?**', async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    const newer = changed && reads++ > 0;
    for (const row of body.inspections)
      row.driverName = newer ? 'Updated Driver' : 'Previous Driver';
    if (changed && !newer) {
      waiting = true;
      await oldWeek.promise;
    }
    await route.fulfill({ response, json: body }).catch(() => {});
  });
  try {
    await enter(page, dsp.name);
    await expect(page.locator('.dvic-driver').first()).toContainText('Previous Driver');
    changed = true;
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await expect.poll(() => waiting).toBe(true);
    status.release();
    await expect(page.locator('.dvic-driver').first()).toContainText('Updated Driver');
    oldWeek.release();
    await page.unrouteAll({ behavior: 'wait' });
    await expect(page.getByText('Previous Driver', { exact: true })).toHaveCount(0);
    await expect(page.locator('.dvic-footer')).toContainText('3 records');
  } finally {
    status.release();
    oldWeek.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('cached DVIC weeks and late reads cannot cross DSP views', async ({ page, dispatch }) => {
  const { owner, dsp } = await seedDvic(dispatch);
  const summit = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Summit Delivery',
  );
  expect(summit).toBeDefined();
  expect(
    (
      await owner.post(`/api/platform/dsps/${summit.id}/features`, {
        feature: 'dvic',
        enabled: true,
      })
    ).status,
  ).toBe(200);
  const status = (await owner.read('/api/dsp/dvic/status')) as DvicStatus;
  const north = (await owner.read('/api/dsp/dvic/inspections?from=2026-09-20&to=2026-09-26'))
    .inspections;
  const hold = gate();
  let refresh = false;
  let waiting = false;
  await page.route('**/api/dsp/dvic/status', (route) => route.fulfill({ json: status }));
  await page.route('**/api/dsp/dvic/inspections?**', async (route) => {
    const isNorth = page.url().includes(dsp.id);
    const inspections = north.map((row: { driverName: string }) => ({
      ...row,
      driverName: isNorth ? 'North Driver' : 'Summit Driver',
    }));
    if (refresh && isNorth) {
      waiting = true;
      await hold.promise;
    }
    await route.fulfill({ json: { inspections, nextCursor: null } }).catch(() => {});
  });
  try {
    await enter(page, dsp.name);
    await expect(page.locator('.dvic-driver').first()).toContainText('North Driver');
    refresh = true;
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await expect.poll(() => waiting).toBe(true);
    await page.getByRole('button', { name: 'Exit view', exact: true }).click();
    await openDsp(page, summit.name);
    await page.getByRole('link', { name: 'DVIC', exact: true }).click();
    await expect(page.locator('.dvic-driver').first()).toContainText('Summit Driver');
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
    await expect(page.getByText('North Driver', { exact: true })).toHaveCount(0);
    await expect(page.locator('.dvic-driver').first()).toContainText('Summit Driver');
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});
