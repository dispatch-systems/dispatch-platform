import { test, expect, login, openDsp } from '../../../../core/shell/tests/support/fixtures.js';

// The seeded DSP has every feature switched on, and its owner holds every permission. Fixture
// mode's Slack sends the browser straight back from installing.

test('an owner adds Slack workspaces from New, each on a card under its name, using what Slack sent back once', async ({
  page,
  dispatch,
}) => {
  await login(page);
  await openDsp(page, 'Northline Logistics');
  await page
    .getByRole('navigation', { name: 'Primary navigation' })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  const settings = page.getByRole('navigation', { name: 'Settings', exact: true });
  await settings.getByRole('link', { name: 'Connections', exact: true }).click();
  const workspaces = page
    .getByRole('article')
    .filter({ has: page.getByRole('term').filter({ hasText: 'Workspace' }) });
  await expect(workspaces).toHaveCount(0);
  const add = async (name = '') => {
    await page.getByLabel('New', { exact: true }).click();
    await page.getByRole('button', { name: 'Slack', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'New Slack connection' });
    await dialog.getByLabel('Name').fill(name);
    await dialog.getByRole('button', { name: 'Connect Slack' }).click();
  };
  await add('Dispatch Ops');
  // Back on Settings' Connections, the install finished under the name it was given, and Slack's
  // code left the address; the tab stays.
  const named = workspaces.filter({ has: page.getByRole('heading', { name: 'Dispatch Ops' }) });
  await expect(named).toContainText('Operations');
  await expect(named).toContainText('Connected');
  // Its logo, which Dispatch keeps a copy of once it's connected, shows as its settings open.
  const owner = await dispatch.client();
  const north = (owner.session.dsps as { id: string; name: string }[]).find(
    (dsp) => dsp.name === 'Northline Logistics',
  )!;
  await owner.select(north.id);
  await expect
    .poll(async () => {
      const { workspaces } = await owner.read('/api/dsp/slack/account');
      return (workspaces as { logo: string | null }[]).some((workspace) => workspace.logo);
    })
    .toBe(true);
  await expect(page).toHaveURL(/tab=connections/);
  expect(page.url()).not.toContain('slackCode');

  // Its settings, on Slack's page of Settings: Dispatch joins a channel there, and the pencil by
  // its name renames it.
  await named.getByRole('link', { name: 'Settings' }).click();
  await expect(page).toHaveURL(/tab=slack&workspace=/);
  const hero = page.getByRole('heading', { level: 2, name: 'Dispatch Ops' });
  await expect(hero).toBeVisible();
  await expect(page.locator('header', { has: hero }).locator('img')).toBeVisible();
  // It moves to Joined at once, every other channel's button left as it was while it saves.
  const joining = '**/channels/*/join';
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(joining, async (route) => {
    await held;
    await route.continue();
  });
  await page.getByRole('button', { name: 'Join photos' }).click();
  const joined = page.getByRole('list', { name: 'Joined', exact: true });
  await expect(joined).toContainText('photos');
  await expect(
    page.getByRole('list', { name: 'Not joined', exact: true }).getByRole('button').first(),
  ).toBeEnabled();
  const answered = page.waitForResponse((response) => response.url().includes('/join'));
  release();
  await answered;
  await page.unroute(joining);
  await expect(joined).toContainText('photos');
  // An integration switched shows as switched as its question closes, before the workspace is
  // read again.
  await page
    .getByRole('navigation', { name: 'Sections' })
    .getByRole('button', { name: 'Integrations', exact: true })
    .click();
  const files = page.getByRole('switch', { name: 'Import Files', exact: true });
  const was = await files.isChecked();
  const reading = '**/api/dsp/slack/account';
  let read = () => {};
  const waiting = new Promise<void>((resolve) => (read = resolve));
  await files.click();
  await page.route(reading, async (route) => {
    await waiting;
    await route.continue();
  });
  await page.getByRole('button', { name: was ? 'Turn off' : 'Turn on', exact: true }).click();
  await expect(files).toBeChecked({ checked: !was });
  read();
  await page.unroute(reading);
  await expect(files).toBeChecked({ checked: !was });
  await page
    .getByRole('navigation', { name: 'Sections' })
    .getByRole('button', { name: 'Channels', exact: true })
    .click();
  await page.getByRole('button', { name: 'Rename' }).click();
  const rename = page.getByRole('dialog', { name: 'Rename connection' });
  await rename.getByLabel('Name').fill('Dispatch Crew');
  await rename.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('heading', { level: 2, name: 'Dispatch Crew' })).toBeVisible();
  await settings.getByRole('link', { name: 'Connections', exact: true }).click();

  // Without a name, it goes by its name in Slack. With two, Slack's page opens one of them.
  await add();
  await expect(workspaces.getByRole('heading', { name: 'Drivers' })).toBeVisible();
  await settings.getByRole('link', { name: 'Slack', exact: true }).click();
  await page.getByLabel(/^Workspace: /).click();
  await page.getByRole('button', { name: 'Drivers', exact: true }).click();
  await expect(page.getByRole('heading', { level: 2, name: 'Drivers' })).toBeVisible();
  await settings.getByRole('link', { name: 'Connections', exact: true }).click();
  const renamed = workspaces.filter({ has: page.getByRole('heading', { name: 'Dispatch Crew' }) });
  await renamed.getByRole('button', { name: 'Disconnect' }).click();
  await page
    .getByRole('dialog', { name: 'Disconnect Dispatch Crew?' })
    .getByRole('button', { name: 'Disconnect' })
    .click();
  await expect(workspaces).toHaveText([/Drivers/]);
});
