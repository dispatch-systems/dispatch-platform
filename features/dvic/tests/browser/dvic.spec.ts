import {
  test,
  expect,
  login,
  openDsp,
  demo,
} from '../../../../core/shell/tests/support/fixtures.js';
import { seedDvic } from '../support/dvic.js';

test('unconfigured collection explains the missing station and Cortex connection', async ({
  page,
  dispatch,
}) => {
  const owner = await dispatch.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await page.getByRole('button', { name: 'Sync now', exact: true }).click();
  await expect(
    page.getByText('Set your station code in the DSP profile before collecting DVIC reports.'),
  ).toBeVisible();
  expect(
    (
      await owner.post('/api/dsp/profile', {
        name: dsp.name,
        abbreviation: 'NLL',
        stationCode: 'TST1',
        timezone: dsp.timezone,
      })
    ).status,
  ).toBe(200);
  await page.reload();
  await page.getByRole('button', { name: 'Sync now', exact: true }).click();
  await expect(
    page.getByText('Connect Cortex in Settings → Connections before syncing DVIC.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Collection settings' }).click();
  await page.getByRole('button', { name: 'Add schedule' }).click();
  await page.getByRole('button', { name: 'Save schedule' }).click();
  await expect(page.getByText('Connect Cortex before enabling DVIC collections.')).toBeVisible();
});

test('the day digest and week grid read the same inspections, with details on desktop and mobile', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch);
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Inspection week' })).toContainText(
    'Sep 20 – Sep 26',
  );
  await expect(page.getByRole('button', { name: 'Latest' })).toBeDisabled();
  // The latest reported day opens first, with its headline numbers and repeat drivers.
  await expect(page.getByRole('heading', { name: 'Sat, Sep 26, 2026' })).toBeVisible();
  await expect(page.locator('.dvic-footer')).toContainText('3 records');
  await expect(page.locator('.dvic-kpi')).toHaveText('Short inspections3');
  await expect(page.getByRole('note')).toHaveText(
    "1 of this day's drivers has three or more short inspections this week: Taylor Brooks.",
  );
  await page
    .getByRole('button', { name: 'View Taylor Brooks, 9:00:00 AM, 34s', exact: true })
    .click();
  const detail = page.getByRole('dialog');
  await expect(detail).toContainText('56s below the minimum · 35–75% of minimum');
  await expect(detail).toContainText('1FIXTURE000000000');
  await expect(detail).toContainText('Sep 27, 2026');
  await expect(detail).toContainText('PASSED');
  await page.keyboard.press('Escape');
  await expect(detail).toHaveCount(0);
  await page
    .getByRole('navigation', { name: 'Inspection days' })
    .getByRole('button')
    .first()
    .click();
  await expect(page.getByRole('heading', { name: 'Sun, Sep 20, 2026' })).toBeVisible();
  await expect(page.locator('.dvic-footer')).toContainText('2 records');
  await page.getByRole('textbox', { name: 'Search drivers' }).fill('Taylor');
  await expect(page.getByText('No matching inspections', { exact: true })).toBeVisible();
  // Filters scope both tabs.
  await page.getByRole('tab', { name: 'Week' }).click();
  await expect(page.locator('.dvic-footer')).toContainText('3 records this week');
  await expect(page.locator('.dvic-grid tbody tr')).toHaveCount(1);
  await page.getByLabel('Vehicle type').selectOption('dot');
  await expect(page.locator('.dvic-footer')).toContainText('1 record this week');
  await page.getByLabel('Vehicle type').selectOption('');
  await page.getByRole('textbox', { name: 'Search drivers' }).fill('');
  await expect(page.locator('.dvic-footer')).toContainText('19 records this week');
  await expect(page.locator('.dvic-grid tbody tr')).toHaveCount(8);
  await expect(page.locator('.dvic-grid-total').nth(1)).toHaveText('3');
  // The shortest inspection that day opens from its cell.
  const cell = page.getByRole('button', { name: 'Taylor Brooks, Sat, Sep 26: 34s', exact: true });
  await expect(cell).toHaveText('34s');
  await cell.click();
  await expect(page.getByRole('dialog')).toContainText('56s below the minimum');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Previous week' }).click();
  await expect(page.getByRole('group', { name: 'Inspection week' })).toContainText(
    'Sep 13 – Sep 19',
  );
  await expect(page.getByText('No short inspections stored', { exact: true })).toBeVisible();
  await expect(page.locator('.dvic-cell')).toHaveCount(0);
  await page.getByRole('button', { name: 'Latest' }).click();
  await expect(page.locator('.dvic-footer')).toContainText('19 records this week');
  await expect(page.getByRole('button', { name: 'Latest' })).toBeDisabled();
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole('heading', { name: 'DVIC', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('tab', { name: 'Day' }).click();
  await expect(page.locator('.dvic-footer')).toContainText('3 records');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.locator('.dvic-item').first().click();
  await expect(page.getByRole('dialog')).toContainText('Inspection duration');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('a full week follows the server cursor and does not present a failed page as zero records', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch, 501);
  const cursors: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/dvic/inspections?')) cursors.push(request.url());
  });
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await page.getByRole('tab', { name: 'Week' }).click();
  await expect(page.locator('.dvic-footer')).toContainText('501 records this week');
  expect(cursors.some((url) => new URL(url).searchParams.has('after'))).toBe(true);
  await page.route('**/api/dsp/dvic/inspections?**', (route) =>
    route.fulfill({
      status: 503,
      contentType: 'application/json',
      body: JSON.stringify({ error: 'unavailable', message: 'Report database unavailable' }),
    }),
  );
  await page.getByRole('button', { name: 'Previous week' }).click();
  await expect(page.getByText('Report database unavailable', { exact: true })).toBeVisible();
  await expect(page.locator('.dvic-cell')).toHaveCount(0);
  await expect(page.getByText('No short inspections stored', { exact: true })).toHaveCount(0);
  await page.unroute('**/api/dsp/dvic/inspections?**');
  await page.getByRole('button', { name: 'Retry inspections' }).click();
  await expect(page.getByText('No short inspections stored', { exact: true })).toBeVisible();
});

test('schedules persist, pause, reject stale edits and delete; manual sync reaches a terminal state', async ({
  page,
  dispatch,
}) => {
  const { dsp, owner } = await seedDvic(dispatch);
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await page.getByRole('button', { name: 'Collection settings' }).click();
  // Unsaved edits leave only through Save or Discard.
  await page.getByRole('button', { name: 'Add schedule' }).click();
  await page.getByLabel('Schedule name').fill('Draft only');
  await page.keyboard.press('Escape');
  const ask = page.getByRole('dialog', { name: 'Save changes?' });
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Discard changes' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.getByRole('button', { name: 'Collection settings' }).click();
  await page.getByRole('button', { name: 'Add schedule' }).click();
  await page.getByLabel('Schedule name').fill('Evening DVIC');
  await page.getByLabel('Frequency').selectOption('interval');
  await page.getByLabel('Every (hours)').fill('3');
  await page.getByLabel('Starting at').fill('18:30');
  await expect(page.locator('output')).not.toHaveText('—');
  await page.getByRole('button', { name: 'Save schedule' }).click();
  await expect(page.getByRole('dialog')).toContainText('Every 3 hours');
  let saved = (await owner.get('/api/dsp/dvic/schedules')).value.schedules[0];
  expect(saved.intervalMinutes).toBe(180);
  await page.getByRole('button', { name: 'Pause Evening DVIC' }).click();
  await expect(page.getByRole('button', { name: 'Resume Evening DVIC' })).toBeVisible();
  await page.getByRole('button', { name: 'Edit Evening DVIC' }).click();
  await page.getByLabel('Starting at').fill('19:00');
  await page.getByRole('button', { name: 'Back' }).click();
  await ask.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByRole('dialog', { name: 'Collection settings' })).toContainText('19:00');
  await page.getByRole('button', { name: 'Edit Evening DVIC' }).click();
  saved = (await owner.get('/api/dsp/dvic/schedules')).value.schedules[0];
  await owner.post('/api/dsp/dvic/schedules/' + saved.id + '/enabled', {
    enabled: true,
    revision: saved.revision,
  });
  await page.getByRole('button', { name: 'Save schedule' }).click();
  await expect(
    page.getByText('This schedule changed in another session. Reload it before saving.'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Save schedule' })).toBeDisabled();
  await page.getByRole('button', { name: 'Reload schedule' }).click();
  await expect(page.getByRole('switch')).toBeChecked();
  await page.getByRole('button', { name: 'Delete schedule', exact: true }).click();
  await page.getByRole('button', { name: 'Delete schedule', exact: true }).click();
  await expect(page.getByText('No schedules yet', { exact: true })).toBeVisible();
  expect((await owner.get('/api/dsp/dvic/schedules')).value.schedules).toHaveLength(0);
  await page.getByRole('button', { name: 'Close dialog' }).click();
  const collected = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/dvic/collect') && response.request().method() === 'POST',
  );
  await page.getByRole('button', { name: 'Sync now', exact: true }).click();
  expect((await collected).status()).toBe(202);
  await expect(page.getByRole('button', { name: 'Sync now', exact: true })).toBeEnabled({
    timeout: 15000,
  });
});

test('DVIC viewers can read without collection controls; a disabled feature has no page or navigation', async ({
  page,
  dispatch,
}) => {
  const { dsp, owner } = await seedDvic(dispatch);
  const member = (await owner.get('/api/dsp/roles')).value.find(
    (role: { name: string }) => role.name === 'Member',
  );
  expect(
    (
      await owner.post('/api/dsp/roles/' + member.id, {
        name: 'Member',
        permissions: ['dvic.view'],
      })
    ).status,
  ).toBe(200);
  const requests: string[] = [];
  page.on('request', (request) => requests.push(request.url()));
  await login(page, demo.member);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await expect(page.locator('.dvic-footer')).toContainText('3 records');
  await expect(page.getByRole('button', { name: 'Sync now', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Collection settings' })).toHaveCount(0);
  expect(requests.some((url) => url.includes('/dvic/schedules'))).toBe(false);
  await owner.post('/api/platform/dsps/' + dsp.id + '/features', {
    feature: 'dvic',
    enabled: false,
  });
  await page.reload();
  await expect(page.getByRole('link', { name: 'DVIC', exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'DVIC', exact: true })).toHaveCount(0);
});
