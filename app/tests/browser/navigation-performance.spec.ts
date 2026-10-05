import fs from 'node:fs';
import { test, expect, login, openDsp } from '../../../core/shell/tests/support/fixtures.js';

const gate = () => {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
};
const routeAsset = (source: string): string =>
  JSON.parse(fs.readFileSync('.build/tooling/build-info.json', 'utf8')).dashboardAssets[source]
    .file;

test('sign-in code starts while the initial session request is still pending', async ({ page }) => {
  const hold = gate();
  const requests: string[] = [];
  page.on('request', (request) => requests.push(new URL(request.url()).pathname));
  await page.route('**/api/session', async (route) => {
    await hold.promise;
    await route.continue();
  });
  try {
    await page.goto('/');
    await expect.poll(() => requests.includes('/api/session')).toBe(true);
    await expect
      .poll(() => requests.includes('/' + routeAsset('../../core/accounts/frontend/index.ts')))
      .toBe(true);
    hold.release();
    await expect(page.getByLabel('Email address')).toBeVisible();
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('cold navigation starts Timecard data alongside code and keeps the current DSP page visible', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const home = page.getByRole('heading', { name: 'Currently under development', exact: true });
  await expect(home).toBeVisible();
  const hold = gate();
  const requests: string[] = [];
  page.on('request', (request) => requests.push(new URL(request.url()).pathname));
  await page.route(
    '**/' + routeAsset('../../features/timecard/frontend/index.ts'),
    async (route) => {
      await hold.promise;
      await route.continue();
    },
  );
  try {
    // Avoid relying on pointer hover: keyboard/programmatic navigation has the same fast path.
    await page
      .getByRole('link', { name: 'Timecard', exact: true })
      .evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page.getByText('Opening page…', { exact: true })).toBeVisible();
    await expect(home).toBeVisible();
    await expect(page.locator('#main-content')).toHaveAttribute('aria-busy', 'true');
    await expect.poll(() => requests.includes('/api/dsp/timecards')).toBe(true);
    hold.release();
    await expect(page.locator('.paycom-timecard-table tbody tr').first()).toBeVisible();
    await expect(home).toHaveCount(0);
    await expect(page.getByText('Opening page…', { exact: true })).toHaveCount(0);
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('leaving a DSP discards its retained page and cancels a pending same-DSP navigation', async ({
  page,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  const home = page.getByRole('heading', { name: 'Currently under development', exact: true });
  await expect(home).toBeVisible();
  const hold = gate();
  await page.route(
    '**/' + routeAsset('../../features/timecard/frontend/index.ts'),
    async (route) => {
      await hold.promise;
      await route.continue();
    },
  );
  try {
    await page
      .getByRole('link', { name: 'Timecard', exact: true })
      .evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page.getByText('Opening page…', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Exit view', exact: true }).click();
    await expect(home).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
    await expect(page.getByRole('heading', { name: 'Timecard', exact: true })).toHaveCount(0);
    await expect(page).toHaveURL(/#dsps$/);
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

for (const outcome of ['success', 'expired'] as const)
  test(`a late ${outcome} role admission cannot replace the latest role or its saved choice`, async ({
    page,
    dispatch,
  }) => {
    const owner = await dispatch.client();
    const dsp = owner.session.dsps.find(
      (item: { name: string }) => item.name === 'Northline Logistics',
    );
    await owner.select(dsp.id);
    const auditor = await owner.post('/api/dsp/roles', { name: 'Auditor', permissions: [] });
    expect(auditor.status).toBe(201);
    await login(page);
    await openDsp(page, dsp.name);
    const home = page.getByRole('heading', { name: 'Currently under development', exact: true });
    const banner = page.getByRole('region', { name: 'DSP viewing mode' });
    const menu = banner.getByLabel('View as role');
    await expect(home).toBeVisible();
    const hold = gate();
    const received = gate();
    await page.route('**/api/session/dsp', async (route) => {
      if (route.request().postDataJSON()?.roleId !== auditor.value.id) {
        await route.continue();
        return;
      }
      const response = await route.fetch();
      received.release();
      await hold.promise;
      if (outcome === 'expired')
        await route.fulfill({
          status: 409,
          contentType: 'application/json',
          body: JSON.stringify({ error: 'dsp_view_expired' }),
        });
      else await route.fulfill({ response });
    });
    try {
      await menu.click();
      await banner.getByRole('button', { name: 'Auditor', exact: true }).click();
      await received.promise;
      // A role boundary must stop showing the previous owner's page before admission resolves.
      await expect(home).toHaveCount(0);
      await expect(banner).toHaveCount(0);
      await page.goBack();
      await expect(page.getByRole('heading', { name: 'DSPs', exact: true })).toBeVisible();
      await openDsp(page, dsp.name);
      await expect(menu).toHaveText('Owner');
      await menu.click();
      await banner.getByRole('button', { name: 'Manager', exact: true }).click();
      await expect(menu).toHaveText('Manager');
      const late = page.waitForResponse(
        (response) =>
          response.url().endsWith('/api/session/dsp') &&
          response.request().postDataJSON()?.roleId === auditor.value.id,
      );
      hold.release();
      await late;
      await page.getByRole('link', { name: 'Timecard', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'Timecard', exact: true })).toBeVisible();
      await expect(menu).toHaveText('Manager');
      await page.reload();
      await expect(menu).toHaveText('Manager');
    } finally {
      hold.release();
      await page.unrouteAll({ behavior: 'wait' });
    }
  });

test('changing the DSP page while its initial admission is pending opens the latest address', async ({
  page,
}) => {
  await login(page);
  const hold = gate();
  const received = gate();
  await page.route('**/api/session/dsp', async (route) => {
    received.release();
    await hold.promise;
    await route.continue();
  });
  try {
    await openDsp(page, 'Northline Logistics');
    await received.promise;
    const dspId = new URL(page.url()).hash.split('/')[1];
    await page.goto(`/#dsp/${dspId}/settings?tab=general`);
    hold.release();
    await expect(page.getByRole('tab', { name: 'Profile', exact: true })).toBeVisible();
    await expect(page.getByRole('region', { name: 'DSP viewing mode' })).toBeVisible();
    await expect(page.locator('.profile-badge .profile-org')).toContainText('Northline Logistics');
  } finally {
    hold.release();
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('a failed admission keeps a useful retry when the same DSP address changes', async ({
  page,
}) => {
  await login(page);
  let failed = false;
  await page.route('**/api/session/dsp', async (route) => {
    if (failed) return route.continue();
    failed = true;
    await route.fulfill({
      status: 400,
      contentType: 'application/json',
      body: JSON.stringify({ error: 'permission_denied' }),
    });
  });
  await openDsp(page, 'Northline Logistics');
  const retry = page.getByRole('button', { name: 'Retry connection', exact: true });
  await expect(retry).toBeVisible();
  const dspId = new URL(page.url()).hash.split('/')[1];
  await page.goto(`/#dsp/${dspId}/settings?tab=general`);
  await expect(retry).toBeVisible();
  await retry.click();
  await expect(page.locator('.profile-badge .profile-org')).toContainText('Northline Logistics');
  await expect(retry).toHaveCount(0);
});
