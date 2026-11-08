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
  // It moves to Joined at once, every other channel's button left as it was while it saves; left
  // before Slack answers, it moves back, the join's answer not moving it again.
  const channels = '**/channels/*/*';
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(channels, async (route) => {
    await held;
    await route.continue();
  });
  const joined = page.getByRole('list', { name: 'Joined', exact: true });
  const notJoined = page.getByRole('list', { name: 'Not joined', exact: true });
  await page.getByRole('button', { name: 'Join photos' }).click();
  await expect(joined).toContainText('photos');
  await expect(notJoined.getByRole('button').first()).toBeEnabled();
  await page.getByRole('button', { name: 'Leave photos' }).click();
  await expect(notJoined).toContainText('photos');
  const left = page.waitForResponse((response) => response.url().endsWith('/leave'));
  release();
  await left;
  await page.unroute(channels);
  await expect(notJoined).toContainText('photos');
  // Both refused, it shows as Slack last had it, not as the first asked.
  let refuse = () => {};
  const refusing = new Promise<void>((resolve) => (refuse = resolve));
  await page.route(channels, async (route) => {
    await refusing;
    await route.fulfill({
      status: 502,
      json: { error: 'refused', message: 'Slack is down' },
    });
  });
  await page.getByRole('button', { name: 'Join photos' }).click();
  await page.getByRole('button', { name: 'Leave photos' }).click();
  await expect(notJoined).toContainText('photos');
  refuse();
  await expect(page.getByText('Slack is down')).toBeVisible();
  await expect(notJoined).toContainText('photos');
  await page.unroute(channels);
  await page.getByRole('button', { name: 'Join photos' }).click();
  await expect(joined).toContainText('photos');
  // An integration switched shows as switched as its question closes, while the switch saves.
  await page
    .getByRole('navigation', { name: 'Sections' })
    .getByRole('button', { name: 'Integrations', exact: true })
    .click();
  const files = page.getByRole('switch', { name: 'Import Files', exact: true });
  const was = await files.isChecked();
  const switching = '**/integrations/*';
  let switched = () => {};
  const holding = new Promise<void>((resolve) => (switched = resolve));
  await page.route(switching, async (route) => {
    await holding;
    await route.continue();
  });
  await files.click();
  await page.getByRole('button', { name: was ? 'Turn off' : 'Turn on', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(files).toBeChecked({ checked: !was });
  const answered = page.waitForResponse((response) => response.url().includes('/integrations/'));
  switched();
  await answered;
  await page.unroute(switching);
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
