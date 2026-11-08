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
  // Switched back and forth while the first is held, the last asked is what's saved: it isn't
  // sent until the first is answered, so it can't reach the server first.
  let saveBoth = () => {};
  const both = new Promise<void>((resolve) => (saveBoth = resolve));
  let switches = 0;
  await page.route(switching, async (route) => {
    switches += 1;
    if (switches === 1) await both;
    await route.continue();
  });
  const turn = async (to: boolean) => {
    await files.click();
    await page.getByRole('button', { name: to ? 'Turn on' : 'Turn off', exact: true }).click();
    await expect(files).toBeChecked({ checked: to });
  };
  await turn(was);
  await turn(!was);
  const second = page.waitForResponse(
    (response) =>
      response.url().includes('/integrations/') && response.request().postDataJSON()?.on === !was,
  );
  saveBoth();
  await second;
  await page.unroute(switching);
  await page.reload();
  await page
    .getByRole('navigation', { name: 'Sections' })
    .getByRole('button', { name: 'Integrations', exact: true })
    .click();
  await expect(files).toBeChecked({ checked: !was });
  // One refused shows what was saved last.
  await page.route(switching, (route) =>
    route.fulfill({ status: 409, json: { error: 'refused', message: 'Not now' } }),
  );
  await files.click();
  await page.getByRole('button', { name: was ? 'Turn on' : 'Turn off', exact: true }).click();
  await expect(page.getByText('Not now')).toBeVisible();
  await expect(files).toBeChecked({ checked: !was });
  await page.unroute(switching);
  // A reading asked for before a switch's answer, though it lands after, doesn't undo it.
  const canvases = page.getByRole('switch', { name: 'Sync Canvases', exact: true });
  const before = await canvases.isChecked();
  const reading = '**/api/dsp/slack/account';
  const heldReads: (() => void)[] = [];
  // Each is answered by the server as it's asked, and handed over only when let go.
  await page.route(reading, async (route) => {
    const response = await route.fetch();
    await new Promise<void>((resolve) => heldReads.push(resolve));
    await route.fulfill({ response });
  });
  const firstSaved = page.waitForResponse((response) => response.url().includes('/integrations/'));
  await files.click();
  await page.getByRole('button', { name: was ? 'Turn on' : 'Turn off', exact: true }).click();
  await firstSaved;
  await expect.poll(() => heldReads.length).toBeGreaterThan(0);
  const asked = heldReads.length;
  const secondSaved = page.waitForResponse((response) => response.url().includes('/integrations/'));
  await canvases.click();
  await page.getByRole('button', { name: before ? 'Turn off' : 'Turn on', exact: true }).click();
  await secondSaved;
  for (const read of heldReads.slice(0, asked)) read();
  await expect(canvases).toBeChecked({ checked: !before });
  await page.unroute(reading);
  for (const read of heldReads.slice(asked)) read();
  await expect(canvases).toBeChecked({ checked: !before });
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
