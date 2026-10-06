import { assessTimecards } from '../support/assessment.js';
import type { Page } from '@playwright/test';
import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

const gate = () => {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
};
const enter = async (page: Page) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Timecard', exact: true }).click();
};

test('refreshing the page discards cached timecards', async ({ page }) => {
  const hold = gate();
  let blocked = false;
  let requested = false;
  await page.route('**/api/dsp/employees*', async (route) => {
    if (blocked) {
      requested = true;
      await hold.promise;
    }
    await route.continue();
  });
  try {
    await enter(page);
    await page.getByRole('tab', { name: 'Employees', exact: true }).click();
    await expect(page.locator('.employee-timecard-section')).toHaveAttribute('aria-busy', 'false');
    blocked = true;
    await page.reload();
    await page.getByRole('tab', { name: 'Employees', exact: true }).click();
    await expect.poll(() => requested).toBe(true);
    await expect(page.getByLabel('Employee directory')).toHaveCount(0);
    await expect(page.getByRole('region', { name: 'Employee details', exact: true })).toHaveCount(
      0,
    );
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('cached names cannot cross DSPs, even when an older request finishes late', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const north = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  const person = (name: string) => ({
    code: 'E001',
    name,
    department: 'Delivery',
    position: 'Driver',
    station: '',
    active: true,
  });
  await page.route('**/api/dsp/paycom/status', async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.connection.enabled = true;
    await route.fulfill({ response, json: body });
  });
  const hold = gate();
  let delayNorth = false;
  let waiting = false;
  await page.route('**/api/dsp/employees**', async (route) => {
    const isNorth = page.url().includes(north.id);
    const employee = person(isNorth ? 'North Driver' : 'Summit Driver');
    const detail = new URL(route.request().url()).pathname.endsWith('/E001');
    if (detail && isNorth && delayNorth) {
      waiting = true;
      await hold.promise;
    }
    await route
      .fulfill({
        json: detail
          ? {
              employee,
              period: { from: '2026-09-06', to: '2026-09-19' },
              collectedAt: '2026-09-20T00:00:00Z',
              syncStatus: null,
              previousPeriod: null,
              nextPeriod: null,
              timecards: assessTimecards([
                {
                  employeeCode: employee.code,
                  date: '2026-09-13',
                  hours: isNorth ? 8 : 4,
                  status: 'Complete',
                  punches: [],
                },
              ]),
            }
          : { employees: [employee], total: 1, collectedAt: null },
      })
      .catch(() => {});
  });
  try {
    await enter(page);
    await page.getByRole('tab', { name: 'Employees', exact: true }).click();
    await expect(page.locator('.employee-timecard-total strong')).toHaveText('8h 00m');
    delayNorth = true;
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await expect.poll(() => waiting).toBe(true);
    await page.getByRole('button', { name: 'Exit view', exact: true }).click();
    await openDsp(page, 'Summit Delivery');
    await page.getByRole('link', { name: 'Timecard', exact: true }).click();
    await page.getByRole('tab', { name: 'Employees', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Summit Driver', exact: true })).toBeVisible();
    await expect(page.locator('.employee-timecard-total strong')).toHaveText('4h 00m');
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
    await expect(page.getByRole('heading', { name: 'North Driver', exact: true })).toHaveCount(0);
    await expect(page.locator('.employee-timecard-total strong')).toHaveText('4h 00m');
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});
