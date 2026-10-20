import {
  test,
  expect,
  login,
  openDsp,
  type Dispatch,
} from '../../../../core/shell/tests/support/fixtures.js';

/** Northline Logistics' owner, the DSP and its profile, with a station and Cortex, as Routes needs. */
async function seedRoutes(dispatch: Dispatch) {
  const owner = await dispatch.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const profile = { name: north.name, abbreviation: 'NLL', stationCode: 'TST1' };
  expect(
    (await owner.post('/api/dsp/profile', { ...profile, timezone: north.timezone })).status,
  ).toBe(200);
  await owner.select(north.id);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  expect(connected.status, connected.body).toBe(200);
  return { owner, north, profile };
}

test('Settings draws route data from its declaration, and a window that deletes stored days asks first', async ({
  page,
  dispatch,
}) => {
  const { owner, north, profile } = await seedRoutes(dispatch);
  // A day from two months ago, collected while every day is kept.
  const day = new Date(Date.now() - 60 * 86_400_000).toISOString().slice(0, 10);
  const queued = await owner.post('/api/dsp/routes/collect', { requestId: 'old', date: day });
  expect(queued.status, queued.body).toBe(202);
  await expect
    .poll(async () => (await owner.read('/api/dsp/routes/days')).days.length, { timeout: 30_000 })
    .toBe(1);

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Data', exact: true })
    .click();
  const section = page.getByRole('region', { name: 'Route data', exact: true });
  const keep = section.getByLabel('Keep route data for');
  await expect(keep).toHaveValue('none');
  const save = section.getByRole('button', { name: 'Save', exact: true });
  await expect(save).toBeDisabled();
  // A custom window outside 30 to 3,650 days can't be saved.
  await keep.selectOption('custom');
  const days = section.getByLabel('Days', { exact: true });
  await days.fill('10');
  await expect(save).toBeDisabled();
  // Unsaved, the page asks before it moves on, as closing the tab does; an edit that can't be
  // saved is discarded, or kept by staying.
  expect(
    await page.evaluate(
      () => !window.dispatchEvent(new Event('beforeunload', { cancelable: true })),
    ),
  ).toBe(true);
  await page.getByRole('link', { name: 'Back', exact: true }).click();
  const ask = page.getByRole('dialog', { name: 'Save changes?' });
  await expect(ask.getByRole('button', { name: 'Save changes' })).toBeDisabled();
  await ask.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(ask).toHaveCount(0);
  await expect(page).toHaveURL(/\/settings\?tab=data$/);
  await expect(days).toHaveValue('10');
  await days.fill('120');
  await expect(save).toBeEnabled();
  // Thirty days passes the stored day: the server's words ask before anything is kept.
  await keep.selectOption('30');
  await save.click();
  const confirm = page.getByRole('dialog', { name: 'Route data', exact: true });
  await expect(confirm).toContainText(
    /Route data from before \w{3} \d{1,2}, \d{4} will be deleted within the hour\. This can’t be undone\./,
  );
  await confirm.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Route data saved', { exact: true })).toBeVisible();
  await expect(save).toBeDisabled();
  await page.reload();
  await expect(
    page.getByRole('region', { name: 'Route data', exact: true }).getByLabel('Keep route data for'),
  ).toHaveValue('30');
  // Keeping every day again deletes nothing, so it saves without asking.
  await page
    .getByRole('region', { name: 'Route data', exact: true })
    .getByLabel('Keep route data for')
    .selectOption('none');
  await page
    .getByRole('region', { name: 'Route data', exact: true })
    .getByRole('button', { name: 'Save', exact: true })
    .click();
  await expect(page.getByText('Route data saved', { exact: true }).first()).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  // Edits begin at the section as read then: a save another session made since isn't
  // overwritten, though the page has read it again meanwhile, as it does when its tab shows.
  await keep.selectOption('90');
  const { revision } = (await owner.read('/api/dsp/settings')).sections.find(
    (each: { id: string }) => each.id === 'routes.retention',
  );
  const other = await owner.post('/api/dsp/settings/routes.retention', {
    values: { days: 365 },
    revision,
  });
  expect(other.status, other.body).toBe(200);
  const reread = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings') && response.request().method() === 'GET',
  );
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await reread;
  await save.click();
  await expect(
    section.getByText(
      'These settings changed in another session and have been reloaded. Make your change again.',
    ),
  ).toBeVisible();
  await expect(keep).toHaveValue('365');
  // The view opening again for the same member and role, as once another session changes the
  // DSP's profile, draws Settings anew as it was: the edit, saved at the revision it began at,
  // and the search.
  await keep.selectOption('90');
  const search = page.getByRole('combobox', { name: 'Search settings', exact: true });
  await search.fill('route');
  const renamed = await owner.post('/api/dsp/profile', {
    ...profile,
    name: 'Northline Freight',
    timezone: north.timezone,
  });
  expect(renamed.status, renamed.body).toBe(200);
  const redrawn = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings') &&
      response.request().method() === 'GET' &&
      response.ok(),
  );
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await redrawn;
  await expect(keep).toHaveValue('90');
  await expect(search).toHaveValue('route');
  const stored = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/dsp/settings/routes.retention') &&
      response.request().method() === 'POST',
  );
  await save.click();
  expect((await stored).status()).toBe(200);
});

test('a member without management permissions never sees the Data page or Driver Match', async ({
  page,
}) => {
  await login(page, 'member@dispatch.test');
  // Settings takes the app's sidebar over once open, so Driver Match is looked for before.
  const settings = page.getByRole('link', { name: 'Settings', exact: true });
  await expect(settings).toBeVisible();
  await expect(page.getByRole('link', { name: /^Driver Match/ })).toHaveCount(0);
  await settings.click();
  const pages = page.getByRole('navigation', { name: 'Settings', exact: true });
  await expect(pages.getByRole('link', { name: 'Profile', exact: true })).toBeVisible();
  await expect(pages.getByRole('link', { name: 'Data', exact: true })).toHaveCount(0);
});

test('a Routes schedule reads yesterday unless it is set to read today so far', async ({
  page,
  dispatch,
}) => {
  const { owner } = await seedRoutes(dispatch);
  const stored = async () => (await owner.read('/api/dsp/routes/schedules')).schedules;

  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('navigation', { name: 'Settings', exact: true })
    .getByRole('link', { name: 'Routes', exact: true })
    .click();
  const collection = page.getByRole('region', { name: 'Collection', exact: true });
  await collection.getByRole('button', { name: 'Add schedule' }).click();
  const day = page.getByRole('combobox', { name: 'Day', exact: true });
  await expect(day).toHaveValue('routes');
  await page.getByLabel('Schedule name').fill('Morning routes');
  await day.selectOption({ label: 'Today so far' });
  await page.getByRole('button', { name: 'Save schedule' }).click();
  await expect(collection).toContainText('Today so far · Daily');
  expect((await stored())[0].collection).toBe('routes_today');
  // Set back, it reads yesterday.
  await page.getByRole('button', { name: 'Edit Morning routes' }).click();
  await expect(day).toHaveValue('routes_today');
  await day.selectOption({ label: 'Yesterday' });
  await page.getByRole('button', { name: 'Save schedule' }).click();
  await expect(collection).toContainText('Yesterday · Daily');
  expect((await stored())[0].collection).toBe('routes');
});
