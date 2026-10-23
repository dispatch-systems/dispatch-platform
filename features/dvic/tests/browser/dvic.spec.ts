import {
  test,
  expect,
  login,
  openDsp,
  demo,
} from '../../../../core/shell/tests/support/fixtures.js';
import { connectDvic, seedDvic } from '../support/dvic.js';

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
  // Its schedule saves while Cortex isn't connected, but stays off till it is.
  await page.getByRole('button', { name: 'Collection settings' }).click();
  const dvic = page.getByRole('region', { name: 'DVIC', exact: true });
  await dvic.getByRole('button', { name: 'Add rule' }).click();
  await dvic.getByLabel('Time 1').fill('18:00');
  await dvic.getByRole('button', { name: 'Save', exact: true }).click();
  const on = dvic.getByRole('switch', { name: 'On schedule' });
  await expect(on).not.toBeChecked();
  await on.click();
  await expect(page.getByText('Connect Cortex before enabling DVIC collections.')).toBeVisible();
  await expect(on).not.toBeChecked();
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
  // The latest collected day opens first, with its headline numbers and repeat drivers.
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
  // The only report collected, Sep 27's, covers Sep 19 to 26: a day before it is unknown, not zero.
  await page.getByRole('tab', { name: 'Day' }).click();
  const strip = page.getByRole('navigation', { name: 'Inspection days' });
  await expect(strip.getByRole('button').last()).toContainText('0 short');
  await expect(strip.getByRole('button').first()).toContainText('Not collected');
  await strip.getByRole('button').first().click();
  await expect(page.getByText('Not collected yet', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Week' }).click();
  await page.getByRole('button', { name: 'Latest' }).click();
  await expect(page.locator('.dvic-footer')).toContainText('19 records this week');
  await expect(page.getByRole('button', { name: 'Latest' })).toBeDisabled();
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

test('the latest collected day opens first even when nobody was short; Today leaves it and Latest returns', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch);
  // Sep 26, the last day Sep 27's report covers, had no short inspections.
  dispatch.database(`dsps/${dsp.id}/data/dvic/dvic.sqlite`, (db) => {
    db.prepare('DELETE FROM dvic_inspections WHERE start_date=?').run('2026-09-26');
    db.prepare('UPDATE dvic_reports SET max_date=?').run('2026-09-25');
  });
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  const latest = page.getByRole('heading', { name: 'Sat, Sep 26, 2026' });
  await expect(latest).toBeVisible();
  await expect(page.getByText('No short inspections on this day', { exact: true })).toBeVisible();
  // Today opens the DSP's current day, even before it has been collected; Latest returns.
  const today = new Intl.DateTimeFormat('en-US', {
    timeZone: dsp.timezone,
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  }).format(new Date());
  await page.getByRole('button', { name: 'Today', exact: true }).click();
  await expect(page.getByRole('heading', { name: today })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Today', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Latest' }).click();
  await expect(latest).toBeVisible();
});

test('a large week opens without downloading every inspection and never shows a failed summary as zero', async ({
  page,
  dispatch,
}) => {
  const { dsp } = await seedDvic(dispatch, 501);
  const inspections: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/dvic/inspections?')) inspections.push(request.url());
  });
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  await page.getByRole('tab', { name: 'Week' }).click();
  await expect(page.locator('.dvic-footer')).toContainText('501 records this week');
  expect(inspections).toEqual([]);
  await page.locator('.dvic-cell').first().click();
  await expect(page.getByRole('dialog')).toContainText('Showing the 50 most recent');
  await expect(page.locator('.dvic-related')).toHaveCount(50);
  await page.getByRole('button', { name: 'Close dialog', exact: true }).click();
  await page.route('**/api/dsp/dvic/summary?**', (route) =>
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
  await page.unroute('**/api/dsp/dvic/summary?**');
  await page.getByRole('button', { name: 'Retry inspections' }).click();
  await expect(page.getByText('No short inspections stored', { exact: true })).toBeVisible();
});

test('its schedule is set on Collectors: saved together, switched off, stale edits refused, unsaved edits asked about; manual sync reaches a terminal state', async ({
  page,
  dispatch,
}) => {
  const { dsp, owner } = await seedDvic(dispatch);
  const stored = async () =>
    (await owner.read('/api/dsp/collectors')).collections.find(
      (collection: { collection: string }) => collection.collection === 'dvic',
    );
  await login(page);
  await openDsp(page, dsp.name);
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  // Its schedule is on Settings' Collectors page, which opens on it.
  await page.getByRole('button', { name: 'Collection settings' }).click();
  await expect(page).toHaveURL(/\/settings\?tab=collectors&collection=dvic$/);
  const dvic = page.getByRole('region', { name: 'DVIC', exact: true });
  const ask = page.getByRole('dialog', { name: 'Save changes?' });
  const collections = page.getByRole('navigation', { name: 'Collections' });
  await dvic.getByRole('button', { name: 'Add rule' }).click();
  await dvic.getByRole('radio', { name: 'Repeat during hours' }).click();
  await dvic.getByLabel('Every', { exact: true }).selectOption({ label: '3 hr' });
  await dvic.getByLabel('From', { exact: true }).fill('18:30');
  await dvic.getByLabel('To', { exact: true }).fill('23:30');
  await expect(dvic.getByRole('listitem')).toHaveCount(5);
  await dvic.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dvic).toContainText('Every day, every 3 hr, 6:30 PM to 11:30 PM');
  let saved = await stored();
  expect(saved.enabled).toBe(true);
  expect([saved.rules[0].every, saved.rules[0].from, saved.rules[0].to]).toEqual([
    180,
    '18:30',
    '23:30',
  ]);
  // Off, nothing of it runs.
  await dvic.getByRole('switch', { name: 'On schedule' }).click();
  await expect(dvic.getByRole('switch', { name: 'On schedule' })).not.toBeChecked();
  expect((await stored()).enabled).toBe(false);
  // Another collection opened with unsaved edits asks first, and Save saves them.
  await dvic.getByRole('button', { name: 'Edit', exact: true }).click();
  await dvic.getByLabel('From', { exact: true }).fill('19:00');
  await collections.getByRole('button', { name: /^Routes/ }).click();
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByRole('region', { name: 'Routes', exact: true })).toBeVisible();
  expect((await stored()).rules[0].from).toBe('19:00');
  await collections.getByRole('button', { name: /^DVIC/ }).click();
  // A save another session passed is refused until the schedule is read again.
  await dvic.getByRole('button', { name: 'Add rule' }).click();
  await dvic.getByLabel('Time 1').fill('06:00');
  saved = await stored();
  expect(
    (
      await owner.post('/api/dsp/collectors/dvic/enabled', {
        enabled: true,
        revision: saved.revision,
      })
    ).status,
  ).toBe(200);
  await dvic.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(
    page.getByText('This schedule changed in another session. Reload it before saving.'),
  ).toBeVisible();
  await expect(dvic.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
  await dvic.getByRole('button', { name: 'Reload schedule' }).click();
  await expect(dvic.getByRole('switch', { name: 'On schedule' })).toBeChecked();
  await dvic.getByRole('button', { name: 'Delete rule' }).click();
  await dvic.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dvic.getByRole('heading', { name: 'Not scheduled' })).toBeVisible();
  expect((await stored()).rules).toHaveLength(0);
  // Leaving the page with unsaved edits asks first, the address staying put meanwhile: opening
  // another from the sidebar, as the DVIC page its Collection settings opened from, once Back
  // has the app's sidebar in again, and the browser's Back alike.
  const draft = () => dvic.getByRole('button', { name: 'Add rule' }).click();
  const settings = /\/settings\?tab=collectors&collection=dvic$/;
  await draft();
  await page.getByRole('complementary').getByRole('button', { name: 'Back', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('link', { name: 'DVIC', exact: true })
    .click();
  await expect(ask).toBeVisible();
  await expect(page).toHaveURL(settings);
  await ask.getByRole('button', { name: 'Discard changes' }).click();
  await expect(page).toHaveURL(/\/dvic$/);
  await page.goBack();
  await draft();
  await page.goBack();
  await expect(ask).toBeVisible();
  await expect(page).toHaveURL(settings);
  // Staying keeps the page, and the browser's history as it was: Forward still leads on, asking.
  await page.keyboard.press('Escape');
  await expect(ask).toHaveCount(0);
  // Signing out asks first too, and staying keeps the session.
  await page.locator('.account-menu summary').click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(ask).toHaveCount(0);
  await expect(page).toHaveURL(settings);
  await page.goForward();
  await expect(ask).toBeVisible();
  await expect(page).toHaveURL(settings);
  await ask.getByRole('button', { name: 'Discard changes' }).click();
  await expect(page).toHaveURL(/\/dvic$/);
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

test('a run started elsewhere shows on an open DVIC page as soon as it finishes', async ({
  page,
  dispatch,
}) => {
  const { dsp, owner } = await connectDvic(dispatch);
  await login(page);
  await openDsp(page, dsp.name);
  // The page's second wait for collection updates is the one a finished run wakes.
  const following = page.waitForRequest(
    (request) =>
      request.url().includes('/api/dsp/collection-updates?after=') &&
      !request.url().endsWith('after='),
  );
  await page.getByRole('link', { name: 'DVIC', exact: true }).click();
  const freshness = page.locator('.dvic-freshness');
  await expect(freshness).toContainText('No completed sync yet');
  await following;
  const job = await owner.post('/api/dsp/dvic/collect', {
    requestId: 'started-elsewhere',
    week: '2026-W39',
  });
  expect(job.status, job.body).toBe(202);
  // Idle, the page rereads its status once a minute; the finished run reaches it well before.
  await expect(freshness).toContainText('Synced', { timeout: 20000 });
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
  expect(requests.some((url) => url.includes('/api/dsp/collectors'))).toBe(false);
  await owner.post('/api/platform/dsps/' + dsp.id + '/features', {
    feature: 'dvic',
    enabled: false,
  });
  await page.reload();
  await expect(page.getByRole('link', { name: 'DVIC', exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'DVIC', exact: true })).toHaveCount(0);
});
